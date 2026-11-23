use cloudflare_bridge::{CookieExpiry, CookieParts, CookieValue};

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
    vec![cookie("cf_clearance", value, ".source.test")]
}

fn cookie(name: &str, value: &str, domain: &str) -> PassCookie {
    PassCookie::new(CookieParts {
        name: name.into(),
        value: CookieValue::new(value),
        domain: domain.into(),
        path: "/".into(),
        secure: true,
        http_only: true,
        expires: CookieExpiry::Session,
    })
    .expect("fixture cookie")
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
fn the_same_clearance_under_another_domain_is_still_the_stale_one() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), clearance("old"));
    let id = DocumentId(1);
    check.start(id);
    check.response(id, &response(TARGET, 403, true), Vec::new());
    let moved = vec![cookie("cf_clearance", "old", "source.test")];
    assert_eq!(
        check.response(id, &response(TARGET, 200, false), moved),
        Decision::Unconfirmed,
        "the site sees name and value; attributes do not make an old pass new"
    );
}

#[test]
fn only_a_non_empty_clearance_cookie_is_a_passage() {
    let target = Url::parse(TARGET).unwrap();
    for cookies in [
        vec![cookie("cf_clearance", "", ".source.test")],
        vec![cookie("__cf_bm", "x", ".source.test")],
    ] {
        let mut check = Verification::new(target.clone(), Vec::new());
        let id = DocumentId(1);
        check.start(id);
        check.response(id, &response(TARGET, 403, true), Vec::new());
        assert_eq!(
            check.response(id, &response(TARGET, 200, false), cookies),
            Decision::Unconfirmed
        );
    }
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

fn headers<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| (*value).to_owned())
    }
}

#[test]
fn a_main_response_reads_the_challenge_type_and_download_headers() {
    let url = Url::parse(TARGET).unwrap();
    let read =
        |pairs: &[(&str, &str)]| MainResponse::from_headers(url.clone(), 403, headers(pairs));
    let interstitial = read(&[
        ("cf-mitigated", " Challenge "),
        ("content-type", "TEXT/HTML; charset=utf-8"),
    ]);
    assert!(interstitial.challenge && interstitial.html && !interstitial.attachment);
    assert!(interstitial.is_interstitial());
    let block = read(&[("cf-mitigated", "block"), ("content-type", "text/html")]);
    assert!(!block.challenge && !block.is_interstitial());
    let json = read(&[
        ("cf-mitigated", "challenge"),
        ("content-type", "application/json"),
    ]);
    assert!(json.challenge && !json.html && !json.is_interstitial());
    let download = read(&[
        ("cf-mitigated", "challenge"),
        ("content-type", "text/html"),
        ("content-disposition", "Attachment; filename=x.html"),
    ]);
    assert!(download.attachment && !download.is_interstitial());
    let inline = read(&[("content-disposition", "inline")]);
    assert!(!inline.attachment && !inline.challenge && !inline.html);
}

#[test]
fn matches_target_needs_the_same_page_and_its_query_but_ignores_challenge_tokens() {
    let target = Url::parse(TARGET).unwrap();
    let same = |text: &str| matches_target(&target, &Url::parse(text).unwrap());
    assert!(same("https://source.test/watch/title?ticket=one"));
    assert!(same(
        "https://source.test/watch/title?ticket=one&__cf_chl_tk=abc"
    ));
    assert!(same("https://source.test/watch/title?extra=1&ticket=one"));
    assert!(
        !same("https://other.test/watch/title?ticket=one"),
        "another origin"
    );
    assert!(
        !same("https://source.test/watch/other?ticket=one"),
        "another page"
    );
    assert!(
        !same("https://source.test/watch/title?ticket=two"),
        "another query"
    );
    assert!(
        !same("https://source.test/watch/title"),
        "the query is lost"
    );
    let challenged = Url::parse("https://source.test/p?__cf_chl_rt=1").unwrap();
    assert!(matches_target(
        &challenged,
        &Url::parse("https://source.test/p").unwrap()
    ));
}

#[test]
fn an_unchallenged_cross_origin_http_redirect_returns_its_status_and_location() {
    let target = Url::parse(TARGET).unwrap();
    for (status, dest) in [
        (301, "https://cinevood.garden/watch/title?ticket=one"),
        (302, "http://moved.test/watch"),
        (303, "https://moved.test/see-other"),
        (307, "https://moved.test/temp"),
        (308, "https://moved.test/perm"),
    ] {
        let mut check = Verification::new(target.clone(), Vec::new());
        let id = DocumentId(1);
        check.start(id);
        let mut reply = response(TARGET, status, false);
        reply.redirect = Some(Url::parse(dest).unwrap());
        assert_eq!(
            check.response(id, &reply, Vec::new()),
            Decision::Redirect {
                status,
                location: Url::parse(dest).unwrap(),
            },
            "status {status} -> {dest}"
        );
    }
}

#[test]
fn an_unchallenged_cross_origin_redirect_works_after_a_same_origin_hop() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
    let id = DocumentId(1);
    check.start(id);
    let mut first = response(TARGET, 302, false);
    first.redirect = Some(Url::parse("https://source.test/canonical").unwrap());
    assert_eq!(check.response(id, &first, Vec::new()), Decision::Wait);
    let mut second = response("https://source.test/canonical", 301, false);
    let dest = Url::parse("https://moved.test/canonical").unwrap();
    second.redirect = Some(dest.clone());
    assert_eq!(
        check.response(id, &second, Vec::new()),
        Decision::Redirect {
            status: 301,
            location: dest,
        }
    );
}

#[test]
fn an_unchallenged_redirect_holding_a_clearance_cookie_still_passes_the_cookie() {
    let mut check = Verification::new(Url::parse(TARGET).unwrap(), Vec::new());
    let id = DocumentId(1);
    check.start(id);
    let mut reply = response(TARGET, 301, false);
    reply.redirect = Some(Url::parse("https://moved.test/").unwrap());
    assert_eq!(
        check.response(id, &reply, clearance("held")),
        Decision::Passed(clearance("held"))
    );
}

#[test]
fn redirect_refuses_non_redirect_codes_and_unsafe_targets() {
    let target = Url::parse(TARGET).unwrap();
    let check_reply = |status: u16, dest: &str| {
        let mut check = Verification::new(target.clone(), Vec::new());
        let id = DocumentId(1);
        check.start(id);
        let mut reply = response(TARGET, status, false);
        reply.redirect = Some(Url::parse(dest).unwrap());
        check.response(id, &reply, Vec::new())
    };
    for status in [300, 304] {
        assert_eq!(
            check_reply(status, "https://moved.test/"),
            Decision::Unconfirmed,
            "status {status} is not an HTTP redirect"
        );
    }
    for unsafe_dest in [
        "file:///etc/passwd",
        "ftp://moved.test/file",
        "https://user@moved.test/",
        "https://:secret@moved.test/",
    ] {
        assert_eq!(
            check_reply(301, unsafe_dest),
            Decision::Unconfirmed,
            "unsafe destination {unsafe_dest}"
        );
    }
}

#[test]
fn redirect_refuses_downloads_challenges_and_post_challenge_hops_without_a_fresh_cookie() {
    let target = Url::parse(TARGET).unwrap();
    let with_dest = |status: u16| {
        let mut reply = response(TARGET, status, false);
        reply.redirect = Some(Url::parse("https://moved.test/").unwrap());
        reply
    };
    let mut download = with_dest(301);
    download.attachment = true;
    let mut check = Verification::new(target.clone(), Vec::new());
    check.start(DocumentId(1));
    assert_eq!(
        check.response(DocumentId(1), &download, Vec::new()),
        Decision::Unconfirmed
    );
    let mut challenged = with_dest(302);
    challenged.challenge = true;
    challenged.html = false;
    assert_eq!(
        check.response(DocumentId(1), &challenged, Vec::new()),
        Decision::Unconfirmed
    );
    let mut after_challenge = Verification::new(target, Vec::new());
    after_challenge.start(DocumentId(1));
    after_challenge.response(DocumentId(1), &response(TARGET, 403, true), Vec::new());
    assert_eq!(
        after_challenge.response(DocumentId(1), &with_dest(301), Vec::new()),
        Decision::Unconfirmed,
        "a redirect after a challenge without a fresh cookie proves nothing"
    );
}
