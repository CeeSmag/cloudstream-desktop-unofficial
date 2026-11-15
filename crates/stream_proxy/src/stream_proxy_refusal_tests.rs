//! T2: what the proxy refuses, and how a stream ends: closed, replaced, unreachable, shut down.

use super::*;
use crate::fake_origin::{get, request, FakeOrigin};
use crate::proxy_test_support::*;

#[test]
fn requests_that_are_not_for_an_open_stream_are_refused() {
    let origin = FakeOrigin::start(show);
    let proxy = proxy();
    let url = proxy
        .open(&spec(origin.url("/show/master.m3u8")))
        .expect("opens");
    let port = proxy.port();
    let token = url.split('/').nth(4).expect("a token").to_owned();
    assert_eq!(request("POST", &url, &[]).expect("answers").status, 405);
    assert_eq!(
        request("GET", &url, &[("Host", "evil.test")])
            .expect("answers")
            .status,
        403
    );
    assert_eq!(get(&format!("http://127.0.0.1:{port}/other")).status, 404);
    assert_eq!(
        get(&format!("http://127.0.0.1:{port}/s/{token}/999.ts")).status,
        404
    );
    assert_eq!(
        get(&format!(
            "http://127.0.0.1:{port}/s/{}/0.m3u8",
            "0".repeat(32)
        ))
        .status,
        410
    );
    proxy.close();
    assert_eq!(get(&url).status, 410, "a closed stream is gone");
}

#[test]
fn switching_stream_keeps_the_old_one_until_the_next_opens() {
    let origin = FakeOrigin::start(show);
    let proxy = proxy();
    let first = proxy
        .open(&spec(origin.url("/show/master.m3u8")))
        .expect("opens");
    proxy
        .open(&spec(origin.url("/show/master.m3u8")))
        .expect("opens");
    assert_eq!(
        get(&first).status,
        200,
        "the player may still be reading the old stream"
    );
    proxy
        .open(&spec(origin.url("/show/master.m3u8")))
        .expect("opens");
    assert_eq!(get(&first).status, 410);
}

#[test]
fn a_host_that_cannot_be_reached_is_a_bad_gateway() {
    // A host that takes every connection and hangs up. (A closed port would do too, but Windows
    // takes two seconds to refuse a connection to one.)
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
    let dead = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || listener.incoming().for_each(drop));
    let proxy = proxy();
    let url = proxy
        .open(&StreamSpec {
            url: format!("http://127.0.0.1:{dead}/x.m3u8"),
            ..StreamSpec::default()
        })
        .expect("opens");
    assert_eq!(get(&url).status, 502);
}

#[test]
fn a_stream_that_is_not_http_is_not_opened_and_a_stopped_proxy_opens_nothing() {
    let proxy = proxy();
    let local = StreamSpec {
        url: "file:///C:/video.m3u8".to_owned(),
        ..StreamSpec::default()
    };
    assert!(matches!(
        proxy.open(&local),
        Err(ProxyError::UnsupportedAddress)
    ));
    proxy.shutdown();
    let web = StreamSpec {
        url: "https://a.test/x.m3u8".to_owned(),
        ..StreamSpec::default()
    };
    assert!(matches!(proxy.open(&web), Err(ProxyError::ShutDown)));
    proxy.shutdown();
}

#[test]
fn a_stopped_proxy_no_longer_listens() {
    let proxy = proxy();
    let port = proxy.port();
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
    proxy.shutdown();
    // The listening socket is closed: the port can be taken by somebody else. (Connecting to it
    // would show the same, but Windows takes two seconds to refuse a connection.)
    assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok());
}
