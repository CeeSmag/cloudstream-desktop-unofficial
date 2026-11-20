//! Property-based and unit tests for `core_model` URL redaction, stable keys, and value helpers.

use core_model::{redact_url, stable_key, LinkSearchCursor, LinkSearchUpdate, WatchProgress};
use proptest::prelude::*;

proptest! {
    #[test]
    fn stable_key_is_always_32_lowercase_hex_and_deterministic(input in ".*") {
        let a = stable_key(&input);
        let b = stable_key(&input);
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(a.len(), 32);
        prop_assert!(a.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
    }

    #[test]
    fn redact_url_never_leaks_path_query_userinfo_or_port(
        scheme in prop_oneof![Just("http"), Just("https")],
        user in "[a-zA-Z0-9]{4,12}",
        pass in "[a-zA-Z0-9]{4,12}",
        host in "[a-z]{3,10}\\.[a-z]{2,5}",
        port in 1024u16..65000,
        secret_seg in "[a-zA-Z0-9]{8,16}",
        token in "[a-zA-Z0-9]{10,20}",
    ) {
        let url = format!("{scheme}://{user}:{pass}@{host}:{port}/private/{secret_seg}?token={token}#frag");
        let redacted = redact_url(&url);
        let expected_prefix = format!("{host}#");
        let port_needle = format!(":{port}");
        prop_assert!(redacted.starts_with(&expected_prefix));
        prop_assert!(!redacted.contains(&user));
        prop_assert!(!redacted.contains(&pass));
        prop_assert!(!redacted.contains(&secret_seg));
        prop_assert!(!redacted.contains(&token));
        prop_assert!(!redacted.contains(&port_needle));
        prop_assert!(!redacted.contains('?'));
        prop_assert!(!redacted.contains("frag"));
    }

    #[test]
    fn redact_url_never_panics_on_arbitrary_input(raw in ".*") {
        let out = redact_url(&raw);
        prop_assert!(!out.contains('?'));
    }

    #[test]
    fn watch_progress_fraction_is_bounded_zero_to_one(
        position_ms in -100_000i64..10_000_000i64,
        duration_ms in -10_000i64..10_000_000i64,
    ) {
        let progress = WatchProgress {
            position_ms,
            duration_ms,
            ..WatchProgress::default()
        };
        let frac = progress.fraction_watched();
        prop_assert!((0.0..=1.0).contains(&frac), "fraction out of range: {frac}");
        if duration_ms <= 0 || position_ms <= 0 {
            prop_assert_eq!(frac, 0.0);
        }
    }
}

#[test]
fn link_search_update_cursor_reflects_next_indices() {
    let update = LinkSearchUpdate {
        next_link: 5,
        next_subtitle: 2,
        ..LinkSearchUpdate::default()
    };
    assert_eq!(
        update.cursor(),
        LinkSearchCursor {
            links: 5,
            subtitles: 2,
        }
    );
}
