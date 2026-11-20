//! Unit (T1) and concurrency/race (T2) tests for `repository_discovery`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use core_model::RepositoryEntry;

use crate::discovery_progress::{DiscoveryConfig, WatchProgress};

use super::*;

fn repo(name: &str) -> RepositoryEntry {
    RepositoryEntry {
        url: format!("https://example.test/{name}/repo.json"),
        name: name.to_owned(),
        added_at_ms: 0,
        icon_url: None,
    }
}

fn fast_config() -> DiscoveryConfig {
    DiscoveryConfig {
        poll_every: Duration::from_millis(1),
        watch_at_most: Duration::from_secs(5),
        quiet_polls_before_stop: 3,
        settle_polls_before_catalog: 2,
        max_consecutive_failures: 3,
    }
}

fn single_quiet_config(poll_every: Duration) -> DiscoveryConfig {
    DiscoveryConfig {
        poll_every,
        watch_at_most: Duration::from_secs(5),
        quiet_polls_before_stop: 1,
        settle_polls_before_catalog: 1,
        max_consecutive_failures: 3,
    }
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(2));
    }
}

fn drain_queue(queue: &Mutex<VecDeque<RepositoryEntry>>) -> PollOutcome {
    let items: Vec<RepositoryEntry> = queue.lock().unwrap().drain(..).collect();
    if items.is_empty() {
        PollOutcome::Empty
    } else {
        PollOutcome::Found {
            added: items.len(),
            repositories: items,
        }
    }
}

#[test]
fn watch_progress_stops_after_quiet_polls_even_when_nothing_was_found() {
    let config = fast_config();
    let mut progress = WatchProgress::default();
    for _ in 0..(config.quiet_polls_before_stop - 1) {
        progress.record_empty();
    }
    assert!(!progress.should_stop(&config));
    progress.record_empty();
    assert!(progress.should_stop(&config));
    assert!(!progress.found_any);
}

#[test]
fn watch_progress_discovery_resets_quiet_counter_and_defers_catalog_reload() {
    let config = fast_config();
    let mut progress = WatchProgress::default();
    progress.record_empty();
    progress.record_empty();
    progress.record_discovered(3);
    assert!(progress.found_any);
    assert_eq!(progress.quiet_polls, 0);
    assert!(!progress.take_catalog_reload(config.settle_polls_before_catalog));

    progress.record_empty();
    assert!(!progress.take_catalog_reload(config.settle_polls_before_catalog));
    progress.record_empty();
    assert!(progress.take_catalog_reload(config.settle_polls_before_catalog));
    assert!(!progress.take_catalog_reload(config.settle_polls_before_catalog));
    assert!(!progress.take_final_catalog_reload());
}

#[test]
fn watch_progress_forgets_single_failure_and_stops_after_streak() {
    let config = fast_config();
    let mut progress = WatchProgress::default();
    progress.record_failure();
    progress.record_empty();
    assert_eq!(progress.failures, 0);
    for _ in 0..config.max_consecutive_failures {
        progress.record_failure();
    }
    assert!(progress.should_stop(&config));
}

#[test]
fn watch_progress_retrigger_resets_quiet_and_failure_counters() {
    let config = fast_config();
    let mut progress = WatchProgress::default();
    progress.record_discovered(1);
    progress.record_failure();
    progress.record_failure();
    progress.reset_for_trigger();
    assert_eq!(progress.quiet_polls, 0);
    assert_eq!(progress.failures, 0);
    assert!(progress.found_any);
    assert!(progress.take_final_catalog_reload());
    assert!(!progress.should_stop(&config));
}

#[test]
fn repeated_discoveries_coalesce_into_single_catalog_reload_after_settle() {
    let discovery = RepositoryDiscovery::with_config(fast_config());
    let batches = Arc::new(Mutex::new(VecDeque::from([
        PollOutcome::Found {
            added: 2,
            repositories: vec![repo("a"), repo("b")],
        },
        PollOutcome::Found {
            added: 1,
            repositories: vec![repo("a"), repo("b"), repo("c")],
        },
    ])));
    let updates = Arc::new(Mutex::new(Vec::new()));
    let poll_batches = Arc::clone(&batches);
    let emit_updates = Arc::clone(&updates);

    discovery.trigger_with(
        move || {
            Ok(poll_batches
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(PollOutcome::Empty))
        },
        move |update| emit_updates.lock().unwrap().push(update),
    );

    wait_until("watcher to finish", || !discovery.is_running());
    let recorded = updates.lock().unwrap();
    let added_counts: Vec<usize> = recorded
        .iter()
        .filter_map(|u| match u {
            DiscoveryUpdate::Added { added, .. } => Some(*added),
            DiscoveryUpdate::ReloadCatalog => None,
        })
        .collect();
    let reloads = recorded
        .iter()
        .filter(|u| matches!(u, DiscoveryUpdate::ReloadCatalog))
        .count();
    assert_eq!(added_counts, vec![2, 1]);
    assert_eq!(reloads, 1);
}

#[test]
fn stop_wakes_sleeping_worker_immediately() {
    let config = DiscoveryConfig {
        poll_every: Duration::from_secs(30),
        ..fast_config()
    };
    let discovery = RepositoryDiscovery::with_config(config);
    discovery.trigger_with(|| Ok(PollOutcome::Empty), |_| {});
    wait_until("worker to start", || discovery.is_running());

    let started = Instant::now();
    discovery.stop();
    wait_until("worker to stop promptly", || !discovery.is_running());
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "stop took {:?} instead of waking immediately",
        started.elapsed()
    );
}

#[test]
fn trigger_after_stop_never_spawns_worker() {
    let discovery = RepositoryDiscovery::with_config(fast_config());
    discovery.stop();
    let polls = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&polls);
    discovery.trigger_with(
        move || {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(PollOutcome::Empty)
        },
        |_| {},
    );
    assert!(!discovery.is_running());
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

struct BarrierHarness {
    entered_last_poll: Arc<Barrier>,
    release_last_poll: Arc<Barrier>,
    pending: Arc<Mutex<VecDeque<RepositoryEntry>>>,
    collected: Arc<Mutex<Vec<String>>>,
}

impl BarrierHarness {
    fn new() -> Self {
        Self {
            entered_last_poll: Arc::new(Barrier::new(2)),
            release_last_poll: Arc::new(Barrier::new(2)),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            collected: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn start(&self, discovery: &RepositoryDiscovery) {
        let enter = Arc::clone(&self.entered_last_poll);
        let release = Arc::clone(&self.release_last_poll);
        let pending = Arc::clone(&self.pending);
        let collected = Arc::clone(&self.collected);
        let polls = AtomicUsize::new(0);
        discovery.trigger_with(
            move || {
                if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                    enter.wait();
                    release.wait();
                    return Ok(PollOutcome::Empty);
                }
                Ok(drain_queue(&pending))
            },
            move |update| {
                if let DiscoveryUpdate::Added { repositories, .. } = update {
                    collected
                        .lock()
                        .unwrap()
                        .extend(repositories.into_iter().map(|r| r.name));
                }
            },
        );
    }
}

/// Deterministic interleaving race test: a second trigger arrives while the worker is executing
/// its final quiet poll (right before deciding to exit). The worker must observe the new
/// generation under the lock and keep polling instead of exiting and losing the new repository.
#[test]
fn retrigger_during_final_quiet_poll_does_not_lose_late_repositories() {
    let discovery = RepositoryDiscovery::with_config(single_quiet_config(Duration::from_millis(1)));
    let harness = BarrierHarness::new();
    harness.start(&discovery);

    harness.entered_last_poll.wait();
    harness
        .pending
        .lock()
        .unwrap()
        .push_back(repo("late-mega-repo"));
    discovery.trigger_with(|| Ok(PollOutcome::Empty), |_| {});
    harness.release_last_poll.wait();

    wait_until("late repository collected and worker finished", || {
        !discovery.is_running()
    });
    assert_eq!(*harness.collected.lock().unwrap(), vec!["late-mega-repo"]);
}

struct StressState {
    discovery: RepositoryDiscovery,
    pending: Mutex<VecDeque<RepositoryEntry>>,
    collected: AtomicUsize,
    active_workers: AtomicUsize,
    max_concurrent_workers: AtomicUsize,
}

impl StressState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            discovery: RepositoryDiscovery::with_config(single_quiet_config(
                Duration::from_micros(200),
            )),
            pending: Mutex::new(VecDeque::new()),
            collected: AtomicUsize::new(0),
            active_workers: AtomicUsize::new(0),
            max_concurrent_workers: AtomicUsize::new(0),
        })
    }

    fn enqueue_and_trigger(self: &Arc<Self>, name: String) {
        self.pending.lock().unwrap().push_back(repo(&name));
        let poll_state = Arc::clone(self);
        let emit_state = Arc::clone(self);
        self.discovery.trigger_with(
            move || {
                let now = poll_state.active_workers.fetch_add(1, Ordering::SeqCst) + 1;
                poll_state
                    .max_concurrent_workers
                    .fetch_max(now, Ordering::SeqCst);
                let outcome = drain_queue(&poll_state.pending);
                poll_state.active_workers.fetch_sub(1, Ordering::SeqCst);
                Ok(outcome)
            },
            move |update| {
                if let DiscoveryUpdate::Added { added, .. } = update {
                    emit_state.collected.fetch_add(added, Ordering::SeqCst);
                }
            },
        );
    }
}

/// Multi-threaded stress race test: several threads hammer `trigger_with` while queuing items and
/// workers start and stop across hundreds of cycles. Invariants: at most one worker runs at once,
/// every queued item is collected, and all threads join without deadlock.
#[test]
fn concurrent_triggers_keep_single_worker_and_never_lose_discoveries() {
    let state = StressState::new();
    let threads = 4usize;
    let per_thread = 150usize;
    let total_expected = threads * per_thread;

    let handles: Vec<_> = (0..threads)
        .map(|worker_id| {
            let shared = Arc::clone(&state);
            thread::spawn(move || {
                for seq in 0..per_thread {
                    shared.enqueue_and_trigger(format!("t{worker_id}-{seq}"));
                }
            })
        })
        .collect();
    for handle in handles {
        handle
            .join()
            .expect("trigger thread finished without panic");
    }

    wait_until("all stress-test items collected and worker idle", || {
        !state.discovery.is_running() && state.collected.load(Ordering::SeqCst) == total_expected
    });
    assert_eq!(state.max_concurrent_workers.load(Ordering::SeqCst), 1);
    assert_eq!(state.collected.load(Ordering::SeqCst), total_expected);
}
