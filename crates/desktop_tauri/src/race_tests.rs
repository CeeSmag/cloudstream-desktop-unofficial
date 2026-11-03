//! Race tests: several threads at once against the state that opening, checking and closing a video
//! share. Each asserts something that must hold for every interleaving, and every thread must
//! finish within a time limit, so a deadlock fails the test instead of hanging the run.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use app_services::{PlaybackItem, StreamChecker};
use extension_repo::{HttpProbe, ProbeResponse, RepoError};

use crate::check_gate::{CheckGate, StartDecision, WaitLimits};
use crate::source_manager::SourceManager;

/// How long a test's threads may take; far more than they need, so only a deadlock exceeds it.
pub(crate) const TIME_LIMIT: Duration = Duration::from_secs(20);

/// Waits for the threads, failing the test when one is still running after `limit` (a deadlock).
pub(crate) fn join_within(handles: Vec<JoinHandle<()>>, limit: Duration) {
    let deadline = Instant::now() + limit;
    for handle in handles {
        while !handle.is_finished() {
            assert!(
                Instant::now() < deadline,
                "a thread did not finish in {limit:?}: deadlock"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        handle.join().expect("a test thread panicked");
    }
}

struct Unreachable;

impl HttpProbe for Unreachable {
    fn probe(&self, url: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        Err(RepoError::Download {
            url: url.to_owned(),
            reason: "unreachable".to_owned(),
        })
    }
}

#[test]
fn opening_and_closing_videos_from_many_threads_never_leaves_two_videos_open() {
    const THREADS: usize = 8;
    const ROUNDS: usize = 300;
    let manager = Arc::new(SourceManager::new(
        StreamChecker::new(Arc::new(Unreachable)),
        crate::test_subtitles::unused_loader(),
    ));
    let gates = Arc::new(Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..THREADS)
        .map(|thread| {
            let (manager, gates) = (Arc::clone(&manager), Arc::clone(&gates));
            std::thread::spawn(move || {
                for round in 0..ROUNDS {
                    let gate = manager.begin();
                    gates.lock().expect("lock").push(Arc::clone(&gate));
                    std::thread::yield_now();
                    let _ = manager.open(&gate, PlaybackItem::default());
                    if (thread + round) % 5 == 0 {
                        manager.close();
                    }
                }
            })
        })
        .collect();
    join_within(handles, TIME_LIMIT);

    let gates = gates.lock().expect("lock");
    let still_open = gates.iter().filter(|gate| !gate.is_cancelled()).count();
    assert!(
        still_open <= 1,
        "{still_open} videos are open at once; opening a video must cancel the one before it"
    );
}

#[test]
fn a_video_replaced_while_it_opens_never_takes_the_session_from_the_newer_one() {
    const ROUNDS: usize = 500;
    for round in 0..ROUNDS {
        let manager = Arc::new(SourceManager::new(
            StreamChecker::new(Arc::new(Unreachable)),
            crate::test_subtitles::unused_loader(),
        ));
        let older = manager.begin();
        let opener = {
            let (manager, older) = (Arc::clone(&manager), Arc::clone(&older));
            std::thread::spawn(move || {
                std::thread::yield_now();
                let _ = manager.open(&older, PlaybackItem::default());
            })
        };
        let newer = manager.begin();
        let newer_generation = manager.open(&newer, PlaybackItem::default());
        join_within(vec![opener], TIME_LIMIT);
        let newer_generation = newer_generation.expect("the newest video can always open");
        assert!(
            manager.session().is_current(newer_generation),
            "round {round}: the replaced video took the session over"
        );
    }
}

const STREAMS: usize = 48;

/// Answers to the checks of every stream, from several threads at once.
fn record_from_threads(gate: &Arc<CheckGate>, works: &[bool]) {
    const THREADS: usize = 8;
    let handles: Vec<_> = (0..THREADS)
        .map(|thread| {
            let (gate, works) = (Arc::clone(gate), works.to_vec());
            std::thread::spawn(move || {
                for index in (thread..STREAMS).step_by(THREADS) {
                    std::thread::yield_now();
                    gate.record(index, works.get(index).copied().unwrap_or(false));
                }
            })
        })
        .collect();
    join_within(handles, TIME_LIMIT);
}

/// The stream that should start: the best working one (highest rank, then earliest), or the best
/// one found when none works.
fn best_to_start(ranks: &[i32], works: &[bool]) -> usize {
    let any_works = works.iter().any(|works| *works);
    (0..ranks.len())
        .filter(|index| !any_works || works.get(*index).copied().unwrap_or(false))
        .max_by_key(|index| {
            (
                ranks.get(*index).copied().unwrap_or(0),
                std::cmp::Reverse(*index),
            )
        })
        .unwrap_or(0)
}

#[test]
fn the_start_decision_does_not_depend_on_the_order_the_answers_arrive_in() {
    let never = Duration::from_secs(60);
    for iteration in 0..60 {
        let ranks: Vec<i32> = (0..STREAMS)
            .map(|i| [360, 480, 720, 1080, 2160][(i * 7 + iteration) % 5])
            .collect();
        let works: Vec<bool> = (0..STREAMS)
            .map(|i| (i * 13 + iteration * 5) % 4 != 0)
            .collect();
        let gate = Arc::new(CheckGate::new(WaitLimits {
            first_stream: never,
            max_wait: never,
        }));
        gate.announce(&ranks);
        record_from_threads(&gate, &works);
        gate.discovery_ended(None);

        let best = best_to_start(&ranks, &works);
        assert_eq!(
            gate.wait_for_start(),
            StartDecision::Start(best),
            "iteration {iteration}"
        );
        let progress = gate.progress();
        assert_eq!(progress.checked, STREAMS);
        assert_eq!(progress.working, works.iter().filter(|w| **w).count());
    }
}

#[test]
fn a_waiting_video_is_woken_by_answers_from_other_threads_and_cancelled_by_a_closer() {
    for _ in 0..200 {
        let gate = Arc::new(CheckGate::new(WaitLimits {
            first_stream: Duration::from_secs(60),
            max_wait: Duration::from_secs(60),
        }));
        let waiter = {
            let gate = Arc::clone(&gate);
            std::thread::spawn(move || {
                assert_eq!(gate.wait_for_start(), StartDecision::Cancelled);
            })
        };
        let answerer = {
            let gate = Arc::clone(&gate);
            std::thread::spawn(move || {
                gate.announce(&[720]);
                gate.record(0, false);
            })
        };
        gate.cancel();
        join_within(vec![waiter, answerer], TIME_LIMIT);
    }
}

/// A skip pressed while answers arrive from other threads and the search ends: the video starts on
/// a stream that worked, never on one that failed, and never hangs.
#[test]
fn a_skip_racing_the_answers_and_the_end_of_the_search_starts_on_a_working_stream() {
    for iteration in 0..200 {
        let gate = Arc::new(CheckGate::new(WaitLimits {
            first_stream: Duration::from_secs(60),
            max_wait: Duration::from_secs(60),
        }));
        let ranks: Vec<i32> = (0..STREAMS).map(|i| [480, 720, 1080][i % 3]).collect();
        let works: Vec<bool> = (0..STREAMS).map(|i| i % 5 == iteration % 5).collect();
        gate.announce(&ranks);
        let waiter = {
            let gate = Arc::clone(&gate);
            std::thread::spawn(move || gate.wait_for_start())
        };
        let skipper = {
            let gate = Arc::clone(&gate);
            std::thread::spawn(move || {
                gate.skip();
            })
        };
        record_from_threads(&gate, &works);
        gate.discovery_ended(None);
        join_within(vec![skipper], TIME_LIMIT);
        let decision = waiter.join().expect("the waiting thread");
        let StartDecision::Start(index) = decision else {
            panic!("iteration {iteration}: {decision:?}");
        };
        assert!(
            works[index],
            "iteration {iteration}: started on stream {index}, which does not work"
        );
    }
}
