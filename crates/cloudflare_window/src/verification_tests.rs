use super::*;

const TARGET: &str = "https://source.test/watch/title?ticket=one";

fn response(url: &str, status: u16, challenge: bool) -> MainResponse {
    MainResponse {
        url: Url::parse(url).expect("fixture URL"),
        status,
        challenge,
        html: true,
        attachment: false,
        redirect: None,
    }
}

fn clearance(value: &str) -> Cookies {
    vec![("cf_clearance".into(), value.into())]
}

#[test]
fn passage_redirect_keeps_origin_cookie_before_the_destination_replaces_it() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), clearance("old"));
    let challenge = DocumentId(1);
    let redirect = DocumentId(2);
    check.start(challenge);
    check.response(challenge, &response(TARGET, 403, true), clearance("old"));
    assert_eq!(check.visible_document(), None);
    check.commit(challenge);
    assert_eq!(check.visible_document(), Some(challenge));
    check.start(redirect);
    assert_eq!(check.visible_document(), None);
    let mut hop = response(TARGET, 302, false);
    hop.redirect = Some(Url::parse("https://landing.test/file").unwrap());
    assert_eq!(
        check.response(redirect, &hop, clearance("new")),
        Decision::Passed(clearance("new"))
    );
}

#[test]
fn navigation_stale_events_cannot_reveal_or_complete_a_replacement_document() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
    let previous = DocumentId(10);
    let current = DocumentId(11);
    check.start(previous);
    check.start(current);
    check.commit(previous);
    assert_eq!(
        check.response(previous, &response(TARGET, 200, false), clearance("new")),
        Decision::Wait
    );
    check.response(previous, &response(TARGET, 403, true), clearance("old"));
    assert_eq!(check.visible_document(), None);
}

#[test]
fn passage_rejects_a_stale_cookie_after_the_browser_was_challenged() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), clearance("old"));
    let id = DocumentId(1);
    check.start(id);
    check.response(id, &response(TARGET, 403, true), clearance("old"));
    assert_eq!(
        check.response(id, &response(TARGET, 200, false), clearance("old")),
        Decision::Unconfirmed
    );
    let mut redirect = response(TARGET, 302, false);
    redirect.redirect = Some(Url::parse("https://landing.test/file").unwrap());
    assert_eq!(
        check.response(id, &redirect, clearance("old")),
        Decision::Unconfirmed
    );
}

#[test]
fn passage_rejects_unrelated_redirect_and_allows_same_origin_completion() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
    let id = DocumentId(1);
    check.start(id);
    assert_eq!(
        check.response(
            id,
            &response("https://other.test/watch/title?ticket=one", 200, false),
            clearance("new")
        ),
        Decision::Unconfirmed
    );
    assert_eq!(
        check.response(id, &response(TARGET, 200, false), clearance("new")),
        Decision::Passed(clearance("new"))
    );
}

#[test]
fn cookie_scope_matches_the_path_and_host_boundary() {
    let target = Url::parse(TARGET).unwrap();
    assert!(cookie_matches(&target, ".source.test", "/watch", true));
    assert!(!cookie_matches(&target, "source.test", "/wat", true));
    assert!(!cookie_matches(&target, "other.test", "/watch", false));
    assert!(!cookie_matches(&target, ".test.invalid", "/", false));
    let plain = Url::parse("http://source.test/watch/title").unwrap();
    assert!(!cookie_matches(&plain, "source.test", "/", true));
}
