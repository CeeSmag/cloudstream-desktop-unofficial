use std::time::Duration;

use url::Url;

use super::{should_slim, slim_address, SlimState};

const AFTER: Duration = Duration::from_secs(20);

fn state(idle_secs: u64) -> SlimState {
    SlimState {
        passed: true,
        slimmed: false,
        fetching: false,
        idle: Duration::from_secs(idle_secs),
    }
}

fn address(text: &str) -> Option<String> {
    slim_address(&Url::parse(text).expect("url")).map(String::from)
}

#[test]
fn the_small_page_is_robots_txt_on_the_same_origin() {
    assert_eq!(
        address("https://site.test:8443/a/b?x=1#f").as_deref(),
        Some("https://site.test:8443/robots.txt")
    );
    assert_eq!(
        address("http://site.test/").as_deref(),
        Some("http://site.test/robots.txt")
    );
}

#[test]
fn only_web_addresses_have_a_small_page() {
    assert_eq!(address("file:///etc/passwd"), None);
    assert_eq!(address("about:blank"), None);
}

#[test]
fn a_quiet_passed_window_is_slimmed_once_the_delay_has_passed() {
    assert!(should_slim(&state(20), AFTER));
    assert!(!should_slim(&state(19), AFTER));
}

#[test]
fn a_window_is_slimmed_only_once() {
    let done = SlimState {
        slimmed: true,
        ..state(500)
    };
    assert!(!should_slim(&done, AFTER));
}

#[test]
fn a_window_that_is_fetching_or_not_passed_is_left_alone() {
    let fetching = SlimState {
        fetching: true,
        ..state(500)
    };
    let checking = SlimState {
        passed: false,
        ..state(500)
    };
    assert!(!should_slim(&fetching, AFTER));
    assert!(!should_slim(&checking, AFTER));
}
