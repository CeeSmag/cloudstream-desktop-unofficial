use std::time::Duration;

use super::{contexts_to_release, ContextLimits, ContextState};

const TTL: Duration = Duration::from_secs(120);
const GRACE: Duration = Duration::from_secs(30);

fn limits(max: usize) -> ContextLimits {
    ContextLimits::new(max, TTL, GRACE)
}

fn passed(key: &str, idle_secs: u64) -> ContextState<'_> {
    ContextState {
        key,
        passed: true,
        fetching: false,
        idle: Duration::from_secs(idle_secs),
    }
}

fn released(states: &[ContextState<'_>], max: usize) -> Vec<String> {
    let mut keys = contexts_to_release(states, &limits(max));
    keys.sort();
    keys
}

#[test]
fn a_window_still_being_checked_is_never_released() {
    let checking = ContextState {
        passed: false,
        ..passed("a", 10_000)
    };
    assert!(released(&[checking], 1).is_empty());
}

#[test]
fn a_window_with_a_fetch_in_flight_is_never_released() {
    let busy = ContextState {
        fetching: true,
        ..passed("a", 10_000)
    };
    assert!(released(&[busy], 1).is_empty());
}

#[test]
fn a_passed_window_unused_for_the_idle_ttl_is_released_even_below_the_cap() {
    assert_eq!(released(&[passed("a", 120)], 8), ["a"]);
    assert!(released(&[passed("a", 119)], 8).is_empty());
}

#[test]
fn a_burst_inside_the_grace_period_keeps_every_window_even_over_the_cap() {
    let burst = [
        passed("a", 1),
        passed("b", 5),
        passed("c", 29),
        passed("d", 0),
    ];
    assert!(released(&burst, 1).is_empty());
}

#[test]
fn over_the_cap_the_least_recently_used_goes_first() {
    let states = [passed("a", 40), passed("b", 90), passed("c", 60)];
    assert_eq!(released(&states, 2), ["b"]);
}

#[test]
fn only_the_excess_is_released() {
    let states = [
        passed("a", 40),
        passed("b", 90),
        passed("c", 60),
        passed("d", 50),
    ];
    assert_eq!(released(&states, 2), ["b", "c"]);
}

#[test]
fn windows_still_being_checked_do_not_count_toward_the_cap() {
    let checking = ContextState {
        passed: false,
        ..passed("pending", 5)
    };
    let states = [checking, passed("a", 40), passed("b", 50)];
    assert!(released(&states, 2).is_empty());
}

#[test]
fn a_window_with_a_fetch_in_flight_counts_toward_the_cap_but_stays() {
    let busy = ContextState {
        fetching: true,
        ..passed("busy", 5)
    };
    let states = [busy, passed("old", 60)];
    assert_eq!(released(&states, 1), ["old"]);
}

#[test]
fn a_window_released_for_age_is_not_also_counted_against_the_cap() {
    let states = [passed("old", 200), passed("a", 40), passed("b", 50)];
    assert_eq!(released(&states, 2), ["old"]);
}

#[test]
fn equally_idle_windows_are_released_in_a_fixed_order() {
    let states = [passed("b", 60), passed("a", 60), passed("c", 60)];
    assert_eq!(released(&states, 2), ["a"]);
}

#[test]
fn a_cap_of_zero_still_keeps_one_window() {
    let states = [passed("a", 40), passed("b", 50)];
    assert_eq!(released(&states, 0), ["b"]);
}

#[test]
fn whatever_the_state_only_idle_passed_windows_are_released_and_the_rest_fit_the_cap() {
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move |bound: u64| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (seed >> 33) % bound
    };
    for _ in 0..2_000 {
        let count = usize::try_from(next(9)).unwrap_or(0);
        let keys: Vec<String> = (0..count).map(|i| format!("k{i}")).collect();
        let states: Vec<ContextState<'_>> = keys
            .iter()
            .map(|key| ContextState {
                key,
                passed: next(4) != 0,
                fetching: next(4) == 0,
                idle: Duration::from_secs(next(300)),
            })
            .collect();
        let max = usize::try_from(next(5)).unwrap_or(1);
        check_invariants(&states, max);
    }
}

fn check_invariants(states: &[ContextState<'_>], max: usize) {
    let out = contexts_to_release(states, &limits(max));
    let by_key = |key: &str| states.iter().find(|s| s.key == key);
    for key in &out {
        let state = by_key(key).expect("a released key exists");
        assert!(
            state.passed && !state.fetching,
            "released a protected window {key}"
        );
    }
    let kept: Vec<&ContextState<'_>> = states
        .iter()
        .filter(|s| s.passed && !out.iter().any(|k| k == s.key))
        .collect();
    let over = kept.len().saturating_sub(max.max(1));
    let unprotected = kept
        .iter()
        .filter(|s| !s.fetching && s.idle >= GRACE)
        .count();
    assert!(
        over == 0 || unprotected == 0,
        "{over} too many kept while {unprotected} could still go"
    );
    assert!(
        kept.iter().all(|s| s.fetching || s.idle < TTL),
        "an expired idle window was kept"
    );
}
