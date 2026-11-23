use crate::pass_fixtures::{now, pass, Spec};

use super::*;

fn entry_with(pass: SavedPass) -> ScopeEntry {
    let mut entry = ScopeEntry::empty(pass.scope().clone());
    entry.save(pass, now());
    entry
}

fn rejected_entry() -> ScopeEntry {
    let mut entry = entry_with(pass("p1", "old"));
    entry.reject(&PassId::parse("p1").expect("id"));
    entry.save(pass("p2", "new"), now());
    entry
}

fn refusal(text: &str) -> String {
    decode_scope(text).expect_err("refused").to_string()
}

#[test]
fn a_scope_survives_the_round_trip_with_its_pass_and_rejections() {
    for entry in [entry_with(pass("p1", "a")), rejected_entry()] {
        let text = encode_scope(&entry);
        assert_eq!(decode_scope(&text).expect("decodes"), entry);
    }
    let only_rejections = {
        let mut entry = entry_with(pass("p1", "a"));
        entry.reject(&PassId::parse("p1").expect("id"));
        entry
    };
    assert_eq!(
        decode_scope(&encode_scope(&only_rejections)).expect("decodes"),
        only_rejections
    );
}

#[test]
fn the_file_names_its_version_and_scope() {
    let text = encode_scope(&entry_with(pass("p1", "a")));
    assert!(
        text.starts_with("{\"version\":1,\"scope\":\".site.test\""),
        "{text}"
    );
}

#[test]
fn a_file_of_another_version_is_refused_before_reading_further() {
    assert!(matches!(
        decode_scope(r#"{"version":2,"anything":["else"]}"#),
        Err(ScopeFileError::UnknownVersion(2))
    ));
    assert!(refusal(r#"{"version":2}"#).contains("version 2"));
}

#[test]
fn each_damaged_part_is_refused() {
    let good = encode_scope(&rejected_entry());
    let cases = [
        (
            good.replacen("\"scope\":\".site.test\"", "\"scope\":\"\"", 1),
            "scope",
        ),
        (
            good.replacen("\"id\":\"p2\"", "\"id\":\"../x\"", 1),
            "pass id",
        ),
        (
            good.replacen(
                "\"issued_for\":\"https://www.site.test/watch\"",
                "\"issued_for\":\"not a url\"",
                1,
            ),
            "issued_for",
        ),
        (good.replacen("\"run\":\"r1\"", "\"run\":\"\"", 1), "run"),
        (
            good.replacen("\"name\":\"cf_clearance\"", "\"name\":\"a b\"", 1),
            "cookie 0",
        ),
        (
            good.replacen("\"user_agent\":\"Agent/1\"", "\"user_agent\":\"\"", 1),
            "user agent",
        ),
        (
            good.replacen("\"scope\":\".site.test\"", "\"scope\":\"site.test\"", 1),
            "another scope",
        ),
        ("{\"version\":1".to_owned(), "not a pass file"),
        ("[]".to_owned(), "not a pass file"),
    ];
    for (text, expected) in cases {
        assert_ne!(text, good, "{expected}: the fixture changed");
        let shown = refusal(&text);
        assert!(shown.contains(expected), "{expected}: {shown}");
    }
}

#[test]
fn times_the_clock_cannot_hold_are_refused() {
    let good = encode_scope(&rejected_entry());
    let too_far = u64::MAX.to_string();
    let obtained = good.replacen(
        &format!("\"obtained_ms\":{}", crate::pass_fixtures::NOW_SECS * 1_000),
        &format!("\"obtained_ms\":{too_far}"),
        1,
    );
    let until_field = good.find("\"until_ms\":").expect("a rejection");
    let until_end = good[until_field..].find('}').expect("end") + until_field;
    let until = format!(
        "{}\"until_ms\":{too_far}{}",
        &good[..until_field],
        &good[until_end..]
    );
    let representable = UNIX_EPOCH
        .checked_add(Duration::from_millis(u64::MAX))
        .is_some();
    for (text, field) in [(obtained, "obtained_ms"), (until, "until_ms")] {
        assert_ne!(text, good);
        if representable {
            let _ = decode_scope(&text);
        } else {
            assert!(refusal(&text).contains(field), "{field}");
        }
    }
}

#[test]
fn a_file_with_more_rejections_than_the_cap_keeps_the_cap() {
    let mut entry = ScopeEntry::empty(CookieDomain::Domain("site.test".into()));
    entry.rejected = (0..MAX_REJECTED_PER_SCOPE + 5)
        .map(|index| RejectedClearance {
            value: CookieValue::new(format!("v{index}")),
            until: now(),
        })
        .collect();
    let read = decode_scope(&encode_scope(&entry)).expect("decodes");
    assert_eq!(read.rejected.len(), MAX_REJECTED_PER_SCOPE);
}

#[test]
fn errors_never_quote_the_file() {
    let secret = "cookie-secret-in-file";
    let texts = [
        format!("{{\"version\":1,\"scope\":{{\"x\":\"{secret}\"}}}}"),
        secret.to_owned(),
        format!("{{\"version\":\"{secret}\"}}"),
    ];
    for text in texts {
        let shown = refusal(&text);
        assert!(!shown.contains(secret), "{shown}");
    }
}

#[test]
fn file_names_are_safe_and_distinct_for_every_scope() {
    let name = |scope: CookieDomain| scope_file_name(&scope);
    assert_eq!(
        name(CookieDomain::Domain("site.test".into())),
        "domain-site.test.json"
    );
    assert_eq!(
        name(CookieDomain::HostOnly("www.site.test".into())),
        "host-www.site.test.json"
    );
    assert_eq!(
        name(CookieDomain::HostOnly("[::1]".into())),
        "host-_5b_3a_3a1_5d.json"
    );
    assert_eq!(
        name(CookieDomain::HostOnly("a_b.test".into())),
        "host-a_5fb.test.json"
    );
    assert_ne!(
        name(CookieDomain::HostOnly("a:b".into())),
        name(CookieDomain::HostOnly("a_3ab".into()))
    );
    let made = Spec::new("p1", "a").domain("www.site.test").build();
    assert_eq!(name(made.scope().clone()), "host-www.site.test.json");
}

#[test]
fn a_pass_obtained_before_1970_is_left_out_rather_than_written_wrong() {
    let entry = entry_with(pass("p1", "a"));
    let mut broken = entry.clone();
    if let Some(saved) = broken.pass.take() {
        let mut parts = saved.parts().clone();
        parts.obtained_at = UNIX_EPOCH - Duration::from_secs(1);
        broken.pass = SavedPass::new(parts).ok();
    }
    let read = decode_scope(&encode_scope(&broken)).expect("decodes");
    assert_eq!(read.pass, None);
}
