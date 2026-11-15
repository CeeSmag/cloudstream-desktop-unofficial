use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::*;
use crate::fake_upstream::{answer, failure, redirect, FakeUpstream};
use crate::stream_spec::StreamSpec;

fn policy() -> HeaderPolicy {
    HeaderPolicy::new(&StreamSpec {
        url: "https://home.test/master.m3u8".to_owned(),
        headers: vec![("Cookie".to_owned(), "a=1".to_owned())],
        referer: "https://site.test/".to_owned(),
    })
}

fn body_of(mut response: OriginResponse) -> String {
    let mut text = String::new();
    response.body.read_to_string(&mut text).expect("a body");
    text
}

fn has_header(hop: &Hop, name: &str) -> bool {
    hop.headers
        .iter()
        .any(|(own, _)| own.eq_ignore_ascii_case(name))
}

#[test]
fn fetch_plain_answer_returns_status_headers_body_and_the_address_asked() {
    let upstream = FakeUpstream::new(|_| answer(200, &[("Content-Type", "text/plain")], b"hello"));
    let response = fetch(&upstream, &policy(), "https://home.test/a.ts", None).expect("answers");
    assert_eq!(
        (response.status, response.final_url.as_str()),
        (200, "https://home.test/a.ts")
    );
    assert_eq!(response.header("content-type"), Some("text/plain"));
    assert_eq!(body_of(response), "hello");
    let hops = upstream.hops();
    assert_eq!(hops.len(), 1);
    assert!(
        has_header(&hops[0], "referer"),
        "the provider's headers go on every request"
    );
}

#[test]
fn fetch_redirects_are_followed_and_the_final_address_is_the_last_one() {
    let upstream = FakeUpstream::new(|hop| match hop.url.as_str() {
        "https://home.test/start.m3u8" => redirect("/moved/list.m3u8"),
        "https://home.test/moved/list.m3u8" => redirect("https://cdn.test/final/list.m3u8"),
        _ => answer(200, &[], b"done"),
    });
    let response =
        fetch(&upstream, &policy(), "https://home.test/start.m3u8", None).expect("answers");
    assert_eq!(response.final_url, "https://cdn.test/final/list.m3u8");
    assert_eq!(upstream.hops().len(), 3);
}

#[test]
fn fetch_a_redirect_to_another_host_does_not_carry_the_cookie_along() {
    let upstream = FakeUpstream::new(|hop| {
        if hop.url.starts_with("https://home.test") {
            redirect("https://cdn.other.test/x.ts")
        } else {
            answer(200, &[], b"x")
        }
    });
    fetch(&upstream, &policy(), "https://home.test/x.ts", None).expect("answers");
    let hops = upstream.hops();
    assert!(has_header(&hops[0], "cookie"));
    assert!(
        !has_header(&hops[1], "cookie"),
        "the cookie belongs to the first host"
    );
    assert!(
        has_header(&hops[1], "referer"),
        "the referer goes everywhere"
    );
}

#[test]
fn fetch_endless_redirects_give_up_after_the_limit() {
    let upstream = FakeUpstream::new(|hop| redirect(&hop.url));
    let result = fetch(&upstream, &policy(), "https://home.test/loop", None);
    assert!(matches!(
        result,
        Err(UpstreamError::TooManyRedirects(MAX_REDIRECTS))
    ));
    assert_eq!(upstream.hops().len(), MAX_REDIRECTS + 1);
}

#[test]
fn fetch_a_redirect_that_cannot_be_followed_is_an_error() {
    let missing = FakeUpstream::new(|_| answer(302, &[], b""));
    let result = fetch(&missing, &policy(), "https://home.test/a", None);
    assert!(matches!(result, Err(UpstreamError::BadRedirect)));
    let local = FakeUpstream::new(|_| redirect("file:///C:/Windows/win.ini"));
    let result = fetch(&local, &policy(), "https://home.test/a", None);
    assert!(matches!(result, Err(UpstreamError::BadRedirect)));
}

#[test]
fn fetch_a_range_the_host_refuses_is_asked_again_without_it() {
    let upstream = FakeUpstream::new(|hop| {
        if hop.range.is_some() {
            answer(403, &[], b"no ranges here")
        } else {
            answer(200, &[], b"the whole file")
        }
    });
    let response = fetch(
        &upstream,
        &policy(),
        "https://home.test/a.ts",
        Some("bytes=5-"),
    )
    .expect("answers");
    assert_eq!(response.status, 200);
    let hops = upstream.hops();
    assert_eq!(hops.len(), 2);
    assert_eq!(hops[0].range.as_deref(), Some("bytes=5-"));
    assert_eq!(hops[1].range, None);
}

#[test]
fn fetch_a_range_the_host_accepts_is_not_asked_twice_and_survives_redirects() {
    let upstream = FakeUpstream::new(|hop| {
        if hop.url.ends_with("/old") {
            redirect("/new")
        } else {
            answer(206, &[("Content-Range", "bytes 5-9/20")], b"56789")
        }
    });
    let response = fetch(
        &upstream,
        &policy(),
        "https://home.test/old",
        Some("bytes=5-9"),
    )
    .expect("answers");
    assert_eq!(response.status, 206);
    assert!(upstream
        .hops()
        .iter()
        .all(|hop| hop.range.as_deref() == Some("bytes=5-9")));
    assert_eq!(upstream.hops().len(), 2);
}

#[test]
fn fetch_an_error_status_without_a_range_is_returned_as_it_is() {
    let upstream = FakeUpstream::new(|_| answer(403, &[], b"denied"));
    let response = fetch(&upstream, &policy(), "https://home.test/a.ts", None).expect("answers");
    assert_eq!(response.status, 403);
    assert_eq!(upstream.hops().len(), 1);
}

#[test]
fn fetch_a_request_that_fails_on_the_way_is_repeated_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let upstream = FakeUpstream::new(move |_| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            failure()
        } else {
            answer(200, &[], b"second time")
        }
    });
    let response = fetch(&upstream, &policy(), "https://home.test/a.ts", None).expect("answers");
    assert_eq!(body_of(response), "second time");
    assert_eq!(upstream.hops().len(), 2);
}

#[test]
fn fetch_a_host_that_keeps_failing_gives_the_error_after_two_tries() {
    let upstream = FakeUpstream::new(|_| failure());
    let result = fetch(&upstream, &policy(), "https://home.test/a.ts", None);
    assert!(matches!(result, Err(UpstreamError::Request { .. })));
    assert_eq!(upstream.hops().len(), 2);
}
