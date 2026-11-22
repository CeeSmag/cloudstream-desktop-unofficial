use std::time::Duration;

use super::{should_stop, IdleInputs};

const fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn quiet(ttl: Option<Duration>) -> IdleInputs {
    IdleInputs {
        ttl,
        since_last_command: secs(600),
        pending_requests: 0,
        since_last_fetch_file: None,
    }
}

#[test]
fn a_helper_quiet_for_longer_than_the_limit_is_stopped() {
    assert!(should_stop(&quiet(Some(secs(300)))));
}

#[test]
fn never_means_the_helper_is_never_stopped() {
    assert!(!should_stop(&quiet(None)));
}

#[test]
fn a_helper_that_took_a_command_recently_is_kept() {
    let inputs = IdleInputs {
        since_last_command: secs(299),
        ..quiet(Some(secs(300)))
    };
    assert!(!should_stop(&inputs));
}

#[test]
fn the_limit_itself_counts_as_idle() {
    let inputs = IdleInputs {
        since_last_command: secs(300),
        ..quiet(Some(secs(300)))
    };
    assert!(should_stop(&inputs));
}

#[test]
fn a_check_the_viewer_may_still_be_doing_keeps_the_helper() {
    let inputs = IdleInputs {
        pending_requests: 1,
        ..quiet(Some(secs(1)))
    };
    assert!(!should_stop(&inputs));
}

#[test]
fn a_browser_fetch_the_helper_answered_recently_keeps_it() {
    let inputs = IdleInputs {
        since_last_fetch_file: Some(secs(10)),
        ..quiet(Some(secs(300)))
    };
    assert!(!should_stop(&inputs));
    let old = IdleInputs {
        since_last_fetch_file: Some(secs(900)),
        ..quiet(Some(secs(300)))
    };
    assert!(should_stop(&old));
}
