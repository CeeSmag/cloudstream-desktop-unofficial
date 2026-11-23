//! T2: what the proxy refuses, and how a stream ends: closed, replaced, unreachable, shut down.

use super::*;
use crate::fake_origin::{get, request, Answer, FakeOrigin};
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

/// Sends `bytes` on a raw connection to the proxy, closes the sending side and returns the
/// status of the answer (0 when there was none).
fn raw_status(port: u16, bytes: &[u8]) -> u16 {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("timeout");
    stream.write_all(bytes).expect("sent");
    stream
        .shutdown(std::net::Shutdown::Write)
        .expect("half closed");
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    String::from_utf8_lossy(&answer)
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or(0)
}

#[test]
fn a_request_the_proxy_cannot_read_is_answered_with_why() {
    let proxy = proxy();
    let port = proxy.port();
    assert_eq!(raw_status(port, b"NONSENSE\r\n\r\n"), 400, "not understood");
    let mut huge = b"GET /x HTTP/1.1\r\nX-Pad: ".to_vec();
    huge.resize(crate::http_request::MAX_HEAD_BYTES + 100, b'p');
    huge.extend_from_slice(b"\r\n\r\n");
    assert_eq!(raw_status(port, &huge), 431, "too large");
    assert_eq!(
        raw_status(port, b"GET /x HTTP/1.1\r\nHo"),
        408,
        "never finished"
    );
}

#[test]
fn dropping_the_proxy_shuts_it_down() {
    let origin = FakeOrigin::start(show);
    let proxy = proxy();
    let url = proxy
        .open(&spec(origin.url("/show/master.m3u8")))
        .expect("opens");
    assert_eq!(get(&url).status, 200);
    drop(proxy);
    assert!(
        request("GET", &url, &[]).is_err(),
        "nothing may answer on the port of a dropped proxy"
    );
}

#[test]
fn a_large_playlist_is_rewritten_and_one_past_the_limit_is_refused() {
    let limit = usize::try_from(crate::playlist_response::MAX_PLAYLIST_BYTES).expect("fits");
    let playlist = |length: usize| {
        let mut text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\nseg.ts\n#X-PAD:".to_owned();
        text.push_str(&"p".repeat(length.saturating_sub(text.len() + 15)));
        text.push_str("\n#EXT-X-ENDLIST\n");
        text
    };
    let (big, too_big) = (playlist(2 * 1024 * 1024), playlist(limit + 4096));
    let origin = FakeOrigin::start(move |request| match request.target.as_str() {
        "/big.m3u8" => Answer::ok(big.clone()),
        "/too-big.m3u8" => Answer::ok(too_big.clone()),
        _ => Answer::status(404),
    });
    let proxy = proxy();
    let fine = get(&proxy.open(&spec(origin.url("/big.m3u8"))).expect("opens"));
    assert_eq!(fine.status, 200);
    assert_eq!(
        media_lines(&fine.text()).len(),
        1,
        "the piece was rewritten"
    );
    let refused = get(&proxy
        .open(&spec(origin.url("/too-big.m3u8")))
        .expect("opens"));
    assert_eq!(
        refused.status, 502,
        "a playlist past the limit is not a playlist"
    );
}

#[test]
fn a_piece_asked_for_with_a_range_is_fetched_from_the_host_once() {
    let origin = FakeOrigin::start(show);
    let proxy = proxy();
    let master = get(&proxy
        .open(&spec(origin.url("/show/master.m3u8")))
        .expect("opens"));
    let variant = get(&media_lines(&master.text())[0]);
    let piece = media_lines(&variant.text())[0].clone();
    let answer = request("GET", &piece, &[("Range", "bytes=0-")]).expect("answers");
    assert_eq!(answer.body, b"SEGMENT-ONE");
    let asked = origin
        .requests()
        .iter()
        .filter(|request| request.target == "/show/v/seg1.jpg")
        .count();
    assert_eq!(
        asked, 1,
        "a rate-limited host must not be asked twice for one piece"
    );
}
