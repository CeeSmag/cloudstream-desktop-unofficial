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

#[test]
fn a_browser_that_is_never_challenged_is_clear_and_hands_over_no_cookies() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
    let id = DocumentId(1);
    check.start(id);
    assert_eq!(
        check.response(id, &response(TARGET, 200, false), Vec::new()),
        Decision::BrowserClear
    );
}

#[test]
fn a_clean_load_that_holds_a_clearance_cookie_still_hands_the_cookie_over() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
    let id = DocumentId(1);
    check.start(id);
    assert_eq!(
        check.response(id, &response(TARGET, 200, false), clearance("kept")),
        Decision::Passed(clearance("kept"))
    );
}

#[test]
fn clear_needs_the_same_origin_a_success_status_and_a_document_not_a_download() {
    let target = Url::parse(TARGET).unwrap();
    let verdict = |reply: MainResponse| {
        let mut check = Verification::new(target.clone(), Vec::new());
        let id = DocumentId(1);
        check.start(id);
        check.response(id, &reply, Vec::new())
    };
    assert_eq!(
        verdict(response(
            "https://other.test/watch/title?ticket=one",
            200,
            false
        )),
        Decision::Unconfirmed
    );
    assert_eq!(
        verdict(response(TARGET, 403, false)),
        Decision::Unconfirmed,
        "a block is not a passage"
    );
    assert_eq!(
        verdict(response(TARGET, 429, false)),
        Decision::Unconfirmed,
        "nor a rate limit"
    );
    assert_eq!(verdict(response(TARGET, 503, false)), Decision::Unconfirmed);
    let mut download = response(TARGET, 200, false);
    download.attachment = true;
    assert_eq!(verdict(download), Decision::Unconfirmed);
    let mut hop = response(TARGET, 301, false);
    hop.redirect = Some(Url::parse("https://source.test/watch/other").unwrap());
    assert_eq!(
        verdict(hop),
        Decision::Wait,
        "a same-origin hop is still in progress"
    );
}

#[test]
fn a_challenge_followed_by_no_cookie_is_never_clear() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
    let id = DocumentId(1);
    check.start(id);
    check.response(id, &response(TARGET, 403, true), Vec::new());
    assert_eq!(
        check.response(id, &response(TARGET, 200, false), Vec::new()),
        Decision::Unconfirmed,
        "the site did challenge the browser, so a missing cookie proves nothing"
    );
}

#[test]
fn a_site_that_answers_not_found_after_the_check_is_passed_not_failed() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
    let (challenge, answer) = (DocumentId(1), DocumentId(2));
    check.start(challenge);
    check.response(challenge, &response(TARGET, 403, true), Vec::new());
    check.start(answer);
    assert_eq!(
        check.response(answer, &response(TARGET, 404, false), clearance("fresh")),
        Decision::Passed(clearance("fresh")),
        "the gate was passed; the page itself does not exist"
    );
}

#[test]
fn a_retry_of_a_page_that_does_not_exist_still_hands_over_the_clearance_it_holds() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), clearance("held"));
    let id = DocumentId(1);
    check.start(id);
    assert_eq!(
        check.response(id, &response(TARGET, 404, false), clearance("held")),
        Decision::Passed(clearance("held"))
    );
}

#[test]
fn a_page_that_does_not_exist_on_a_site_that_never_challenges_the_browser_is_clear() {
    for status in [404, 410, 500] {
        let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
        let id = DocumentId(1);
        check.start(id);
        assert_eq!(
            check.response(id, &response(TARGET, status, false), Vec::new()),
            Decision::BrowserClear,
            "status {status}: the site answered, the browser is not challenged"
        );
    }
}
