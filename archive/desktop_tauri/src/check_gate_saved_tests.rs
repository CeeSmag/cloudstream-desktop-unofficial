//! The gate with sources saved from an earlier time: trusted ones play at once, others are tested
//! first for a short while, and when none of them works the video waits for the search as usual.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;

fn gate(first_ms: u64, max_ms: u64) -> Arc<CheckGate> {
    Arc::new(CheckGate::new(WaitLimits {
        first_stream: Duration::from_millis(first_ms),
        max_wait: Duration::from_millis(max_ms),
    }))
}

fn after(pause_ms: u64, action: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(pause_ms));
        action();
    });
}

const TEST_CAP: Duration = Duration::from_secs(3);

#[test]
fn trusted_saved_sources_play_at_once_without_any_answer_or_search_result() {
    let gate = gate(30_000, 60_000);
    gate.announce_saved(&[1080, 720]);
    let progress = gate.trust_saved(0);
    let started = Instant::now();
    assert_eq!(gate.wait_for_start(), StartDecision::Start(0));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(progress.saved, SavedSources::Trusted);
    assert!(gate.progress().discovering, "the search goes on");
}

#[test]
fn the_first_tested_saved_source_that_works_starts_the_video_while_the_search_goes_on() {
    let gate = gate(30_000, 60_000);
    gate.announce_saved(&[1080, 720, 480]);
    gate.test_saved_first(3, TEST_CAP);
    let later = Arc::clone(&gate);
    after(30, move || {
        later.record(0, false);
        later.record(1, true);
    });
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
    assert!(gate.progress().discovering);
    assert_eq!(gate.progress().saved, SavedSources::Testing);
}

#[test]
fn a_working_source_outside_the_tested_ones_does_not_start_the_video() {
    let gate = gate(30_000, 60_000);
    gate.announce_saved(&[1080, 720, 480, 360]);
    gate.test_saved_first(2, TEST_CAP);
    gate.record(3, true);
    let later = Arc::clone(&gate);
    after(60, move || {
        later.record(0, true);
    });
    let started = Instant::now();
    assert_eq!(gate.wait_for_start(), StartDecision::Start(0));
    assert!(started.elapsed() >= Duration::from_millis(50));
}

#[test]
fn when_every_tested_source_is_broken_the_video_waits_for_the_search_as_usual() {
    let gate = gate(30_000, 60_000);
    gate.announce_saved(&[1080, 720]);
    gate.test_saved_first(2, TEST_CAP);
    let later = Arc::clone(&gate);
    after(30, move || {
        later.record(0, false);
        later.record(1, false);
    });
    let search = Arc::clone(&gate);
    after(150, move || {
        search.announce(&[480]);
        search.record(2, true);
        search.discovery_ended(None);
    });
    let started = Instant::now();
    assert_eq!(gate.wait_for_start(), StartDecision::Start(2));
    assert!(
        started.elapsed() >= Duration::from_millis(140),
        "must wait for the search, not play a broken saved source"
    );
    assert_eq!(gate.progress().saved, SavedSources::OutOfDate);
}

#[test]
fn when_the_saved_sources_do_not_answer_in_time_the_test_ends_and_the_search_is_awaited() {
    let gate = gate(30_000, 60_000);
    gate.announce_saved(&[1080]);
    gate.test_saved_first(1, Duration::from_millis(50));
    let search = Arc::clone(&gate);
    after(200, move || {
        search.record(0, false); // the slow check of the saved source finally answers
        search.announce(&[720]);
        search.record(1, true);
        search.discovery_ended(None);
    });
    let started = Instant::now();
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
    assert!(started.elapsed() >= Duration::from_millis(190));
    assert_eq!(gate.progress().saved, SavedSources::OutOfDate);
}

#[test]
fn saved_sources_do_not_count_as_an_answer_of_the_provider() {
    let gate = gate(80, 5_000);
    gate.announce_saved(&[1080]);
    gate.test_saved_first(1, Duration::from_millis(20));
    gate.record(0, false);
    let started = Instant::now();
    let StartDecision::Failed(reason) = gate.wait_for_start() else {
        panic!("a silent provider and a dead saved source must be an error");
    };
    assert!(reason.contains("did not answer"), "{reason}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn closing_the_video_while_saved_sources_are_tested_ends_the_wait() {
    let gate = gate(30_000, 60_000);
    gate.announce_saved(&[1080]);
    gate.test_saved_first(1, TEST_CAP);
    let later = Arc::clone(&gate);
    after(30, move || later.cancel());
    assert_eq!(gate.wait_for_start(), StartDecision::Cancelled);
}

#[test]
fn skipping_during_the_test_plays_a_working_source_at_once() {
    let gate = gate(30_000, 60_000);
    gate.announce_saved(&[1080, 720]);
    gate.test_saved_first(1, TEST_CAP);
    gate.record(1, true);
    gate.skip();
    assert_eq!(gate.wait_for_start(), StartDecision::Start(1));
}
