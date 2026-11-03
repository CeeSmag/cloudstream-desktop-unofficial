use std::sync::Arc;
use std::time::Duration;

use super::*;

const LONG: WaitLimits = WaitLimits {
    first_stream: Duration::from_secs(30),
    max_wait: Duration::from_secs(60),
};

fn gate() -> Arc<CheckGate> {
    Arc::new(CheckGate::new(LONG))
}

fn limited(first_ms: u64, max_ms: u64) -> Arc<CheckGate> {
    Arc::new(CheckGate::new(WaitLimits {
        first_stream: Duration::from_millis(first_ms),
        max_wait: Duration::from_millis(max_ms),
    }))
}

fn after_a_moment(action: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        action();
    });
}

#[test]
fn starts_on_the_best_working_stream_once_everything_is_checked() {
    let gate = gate();
    gate.announce(&[480, 720, 1080]);
    gate.record(0, true);
    gate.record(1, true);
    gate.record(2, false);
    gate.discovery_ended(None);
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

#[test]
fn starts_on_the_best_known_stream_when_none_works() {
    let gate = gate();
    gate.announce(&[360, 1080, 720]);
    for index in 0..3 {
        gate.record(index, false);
    }
    gate.discovery_ended(None);
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

/// The change from the old behavior: a working stream no longer starts playback on its own, not
/// even a high quality one; the wait goes on until the search and the checks are done.
#[test]
fn a_working_stream_does_not_start_playback_while_the_search_or_the_checks_go_on() {
    let gate = gate();
    gate.announce(&[2160, 480]);
    gate.record(0, true);
    let later = Arc::clone(&gate);
    let started = Instant::now();
    after_a_moment(move || {
        later.record(1, true);
        later.discovery_ended(None);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(0));
    assert!(
        started.elapsed() >= Duration::from_millis(25),
        "started before the search and the checks were done"
    );
    assert_eq!(gate.progress().checked, 2);
    assert!(!gate.progress().discovering);
}

#[test]
fn a_better_stream_found_while_waiting_wins() {
    let gate = gate();
    gate.announce(&[480, 1080]);
    gate.record(0, true);
    let later = Arc::clone(&gate);
    after_a_moment(move || {
        later.record(1, true);
        later.discovery_ended(None);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

#[test]
fn skipping_starts_at_once_on_what_is_known_and_does_not_end_the_search() {
    let gate = gate();
    gate.announce(&[480, 480]);
    gate.record(1, true);
    gate.skip();
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
    assert!(!gate.is_cancelled());
    assert!(
        gate.progress().discovering,
        "skipping must not end the search"
    );
}

#[test]
fn skipping_waits_for_the_first_working_stream() {
    let gate = gate();
    gate.announce(&[720, 720]);
    gate.record(0, false);
    gate.skip();
    let later = Arc::clone(&gate);
    after_a_moment(move || {
        later.record(1, true);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

#[test]
fn a_search_that_finds_nothing_fails_with_its_reason() {
    let gate = gate();
    gate.discovery_ended(Some("provider exploded".to_owned()));
    assert_eq!(
        gate.wait_for_start(),
        StartDecision::Failed("provider exploded".to_owned())
    );
    let empty = CheckGate::new(LONG);
    empty.discovery_ended(None);
    assert_eq!(
        empty.wait_for_start(),
        StartDecision::Failed(NO_STREAMS.to_owned())
    );
}

#[test]
fn waits_while_the_search_may_still_find_streams() {
    let gate = gate();
    let later = Arc::clone(&gate);
    after_a_moment(move || {
        later.announce(&[720]);
        later.record(0, true);
        later.discovery_ended(None);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(0));
}

#[test]
fn closing_the_video_ends_the_wait() {
    let gate = gate();
    let later = Arc::clone(&gate);
    after_a_moment(move || later.cancel());
    assert_eq!(gate.wait_for_start(), StartDecision::Cancelled);
    assert!(gate.is_cancelled());
}

#[test]
fn progress_counts_streams_answers_and_working_streams() {
    let gate = gate();
    gate.announce(&[720, 720]);
    gate.announce(&[480, 480]);
    gate.record(0, true);
    let progress = gate.record(1, false);
    assert_eq!(
        progress,
        CheckProgress {
            checked: 2,
            total: 4,
            working: 1,
            skipping: false,
            discovering: true
        }
    );
    assert!(!gate.discovery_ended(None).discovering);
}

#[test]
fn a_provider_that_gives_no_stream_in_time_fails_with_an_error_that_says_so() {
    let gate = limited(40, 5_000);
    let started = Instant::now();
    let StartDecision::Failed(reason) = gate.wait_for_start() else {
        panic!("a silent provider must be an error");
    };
    assert!(reason.contains("did not answer"), "{reason}");
    assert!(started.elapsed() >= Duration::from_millis(35));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn a_stream_that_arrives_before_the_first_stream_limit_keeps_the_wait_going() {
    let gate = limited(300, 5_000);
    let later = Arc::clone(&gate);
    after_a_moment(move || {
        later.announce(&[720]);
        later.record(0, true);
        later.discovery_ended(None);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(0));
}

#[test]
fn at_the_cap_the_best_working_stream_plays_while_the_search_goes_on() {
    let gate = limited(5_000, 60);
    gate.announce(&[480, 1080, 720]);
    gate.record(0, true);
    gate.record(2, true);
    assert_eq!(gate.wait_for_start(), StartDecision::Start(2));
    assert!(gate.progress().discovering);
}

#[test]
fn at_the_cap_the_best_stream_not_yet_known_to_be_broken_plays() {
    let gate = limited(5_000, 40);
    gate.announce(&[1080, 720, 480]);
    gate.record(0, false);
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

#[test]
fn at_the_cap_with_every_stream_broken_the_wait_fails_and_says_what_was_tried() {
    let gate = limited(5_000, 40);
    gate.announce(&[720, 480]);
    gate.record(0, false);
    gate.record(1, false);
    let StartDecision::Failed(reason) = gate.wait_for_start() else {
        panic!("nothing works, so the wait must fail at the cap");
    };
    assert!(reason.contains("No working source"), "{reason}");
    assert!(reason.contains("2 found, 2 checked"), "{reason}");
}

#[test]
fn the_cap_ends_the_wait_even_when_the_first_stream_limit_is_longer() {
    let gate = limited(5_000, 40);
    let started = Instant::now();
    assert!(matches!(gate.wait_for_start(), StartDecision::Failed(_)));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn closing_ends_the_wait_even_with_limits_far_away() {
    let gate = limited(60_000, 120_000);
    let later = Arc::clone(&gate);
    after_a_moment(move || later.cancel());
    let started = Instant::now();
    assert_eq!(gate.wait_for_start(), StartDecision::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(3));
}
