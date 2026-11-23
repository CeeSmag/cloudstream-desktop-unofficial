use crate::test_logs::captured;

use super::*;

const SECRET: &str = "cookie-value-never-logged";

#[test]
fn a_malformed_status_line_is_reported_by_its_word_only() {
    for line in [
        format!("passed\t\t{SECRET}"),
        format!("passed\t{SECRET}"),
        format!("failed\t{SECRET}"),
    ] {
        let (status, logs) = captured(|| status_of(&line));
        assert_eq!(status, None);
        assert!(!logs.contains(SECRET), "{logs}");
        assert!(logs.contains("malformed status line"), "{logs}");
    }
}

#[test]
fn an_unreadable_pass_keeps_the_status_and_hides_the_payload() {
    let line = format!("passed\tsite.test\tr1\t{{\"user_agent\":\"{SECRET}\",\"cookies\":7}}");
    let (status, logs) = captured(|| status_of(&line));
    assert_eq!(
        status,
        Some(HelperStatus::Passed {
            host: "site.test".into(),
            id: "r1".into(),
            issued: None
        })
    );
    assert!(!logs.contains(SECRET), "{logs}");
    assert!(logs.contains("could not be read"), "{logs}");
}

#[test]
fn the_helper_s_own_log_lines_are_logged_at_their_level() {
    let (status, logs) = captured(|| {
        [
            "2026 ERROR boom",
            "2026 WARN careful",
            "2026 DEBUG detail",
            "plain line",
        ]
        .map(status_of)
    });
    assert!(status.iter().all(Option::is_none));
    for (level, text) in [
        ("ERROR", "boom"),
        ("WARN", "careful"),
        ("DEBUG", "detail"),
        ("INFO", "plain line"),
    ] {
        assert!(
            logs.lines()
                .any(|line| line.contains(level) && line.contains(text)),
            "{level} {text}: {logs}"
        );
    }
}

#[test]
fn a_valid_status_is_forwarded_without_a_log_line() {
    let (status, logs) = captured(|| status_of("open\tsite.test\tr1"));
    assert_eq!(
        status,
        Some(HelperStatus::WindowOpen {
            host: "site.test".into(),
            id: "r1".into()
        })
    );
    assert!(logs.is_empty(), "{logs}");
}
