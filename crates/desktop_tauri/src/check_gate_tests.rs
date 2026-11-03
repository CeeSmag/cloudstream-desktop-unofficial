use std::sync::Arc;
use std::time::Duration;

use super::*;

const SHORT: Grace = Grace {
    high_quality: Duration::from_millis(40),
    other: Duration::from_millis(250),
    max_wait: Duration::from_secs(30),
};

fn gate() -> Arc<CheckGate> {
    Arc::new(CheckGate::new(SHORT))
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

#[test]
fn a_high_quality_stream_starts_after_the_short_grace_while_checks_go_on() {
    let gate = gate();
    gate.announce(&[1080, 480, 480]);
    gate.record(0, true);
    assert_eq!(gate.wait_for_start(), StartDecision::Start(0));
    assert!(gate.progress().discovering, "did not wait for the rest");
}

#[test]
fn a_lower_quality_stream_waits_longer_for_something_better() {
    let gate = gate();
    gate.announce(&[480]);
    gate.record(0, true);
    let started = Instant::now();
    assert_eq!(gate.wait_for_start(), StartDecision::Start(0));
    assert!(started.elapsed() >= Duration::from_millis(200));
}

#[test]
fn a_better_stream_found_during_the_grace_wins() {
    let gate = gate();
    gate.announce(&[480, 1080]);
    gate.record(0, true);
    let later = Arc::clone(&gate);
    after_a_moment(move || {
        later.record(1, true);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

#[test]
fn skipping_starts_at_once_on_what_is_known() {
    let gate = gate();
    gate.announce(&[480, 480]);
    gate.record(1, true);
    gate.skip();
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
    assert!(!gate.is_cancelled());
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
    let empty = CheckGate::new(SHORT);
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

fn limited(max_wait_ms: u64) -> Arc<CheckGate> {
    Arc::new(CheckGate::new(Grace {
        high_quality: Duration::from_secs(30),
        other: Duration::from_secs(30),
        max_wait: Duration::from_millis(max_wait_ms),
    }))
}

#[test]
fn the_limit_cuts_the_grace_short() {
    let gate = limited(60);
    gate.announce(&[480, 480]);
    gate.record(1, true);
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

#[test]
fn at_the_limit_the_best_stream_not_yet_known_to_be_broken_plays() {
    let gate = limited(40);
    gate.announce(&[1080, 720, 480]);
    gate.record(0, false);
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

#[test]
fn at_the_limit_a_stream_that_is_still_being_checked_plays() {
    let gate = limited(40);
    gate.announce(&[480, 1080]);
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}

#[test]
fn after_the_limit_the_wait_goes_on_for_the_first_stream() {
    let gate = limited(10);
    let later = Arc::clone(&gate);
    after_a_moment(move || {
        later.announce(&[720]);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(0));
}

#[test]
fn after_the_limit_streams_known_to_be_broken_are_not_chosen() {
    let gate = limited(10);
    gate.announce(&[720]);
    gate.record(0, false);
    let later = Arc::clone(&gate);
    after_a_moment(move || {
        later.announce(&[360]);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}
