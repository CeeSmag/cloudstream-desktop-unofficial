//! T2: the proxy against real sockets, with a fake origin server in place of the stream's host.

use super::*;
use crate::fake_origin::{get, request, Answer, FakeOrigin};
use crate::http_request::HttpRequest;
use crate::proxy_test_support::*;

#[test]
fn a_whole_stream_plays_through_the_proxy_with_the_providers_headers_on_every_request() {
    let origin = FakeOrigin::start(show);
    let proxy = proxy();
    let master_url = proxy
        .open(&spec(origin.url("/show/master.m3u8")))
        .expect("opens");
    let variants = assert_master_is_rewritten(&get(&master_url), &proxy);

    let variant = get(variants.first().expect("a variant"));
    assert_eq!(variant.status, 200, "{}", variant.text());
    assert_eq!(get(&uri_of(&variant.text(), "#EXT-X-KEY")).body, KEY_BYTES);
    let pieces = media_lines(&variant.text());
    assert!(
        pieces.iter().all(|line| line.ends_with(".ts")),
        "disguised names are replaced: {pieces:?}"
    );
    assert_eq!(get(&pieces[0]).body, b"SEGMENT-ONE");
    assert_eq!(get(&pieces[1]).body, b"SEGMENT-TWO");
    assert_provider_headers_everywhere(&origin);
}

fn pieces_host(request: &HttpRequest) -> Answer {
    match (request.target.as_str(), request.header("range")) {
        ("/p.m3u8", _) => Answer::ok("#EXTM3U\n#EXTINF:4,\nf.ts\n"),
        ("/f.ts", Some("bytes=2-5")) => Answer::status(206)
            .with("Content-Range", "bytes 2-5/10")
            .with("Accept-Ranges", "bytes")
            .tap_body(b"2345"),
        ("/f.ts", None) => Answer::ok("0123456789"),
        _ => Answer::status(404),
    }
}

fn first_piece(proxy: &StreamProxy, origin: &FakeOrigin, path: &str) -> String {
    let playlist = get(&proxy
        .open(&StreamSpec {
            url: origin.url(path),
            ..StreamSpec::default()
        })
        .expect("opens"));
    media_lines(&playlist.text()).remove(0)
}

#[test]
fn a_range_request_is_passed_on_and_answered_with_the_part() {
    let origin = FakeOrigin::start(pieces_host);
    let proxy = proxy();
    let piece = first_piece(&proxy, &origin, "/p.m3u8");
    let part = request("GET", &piece, &[("Range", "bytes=2-5")]).expect("answers");
    assert_eq!(part.status, 206);
    assert_eq!(part.header("content-range"), Some("bytes 2-5/10"));
    assert_eq!(part.body, b"2345");
    assert_eq!(
        get(&piece).body,
        b"0123456789",
        "without a range it is the whole piece"
    );
}

#[test]
fn a_host_that_refuses_ranges_still_serves_the_piece() {
    let origin =
        FakeOrigin::start(
            |request| match (request.target.as_str(), request.header("range")) {
                ("/p.m3u8", _) => Answer::ok("#EXTM3U\n#EXTINF:4,\nf.ts\n"),
                ("/f.ts", Some(_)) => Answer::status(403),
                _ => Answer::ok("0123456789"),
            },
        );
    let proxy = proxy();
    let piece = first_piece(&proxy, &origin, "/p.m3u8");
    let reply = request("GET", &piece, &[("Range", "bytes=2-")]).expect("answers");
    assert_eq!(
        (reply.status, reply.body.as_slice()),
        (200, b"0123456789".as_slice())
    );
}

#[test]
fn a_redirected_playlist_resolves_its_pieces_against_the_address_it_ended_at() {
    let origin = FakeOrigin::start(|request| match request.target.as_str() {
        "/go.m3u8" => Answer::status(302).with("Location", "/real/dir/list.m3u8"),
        "/real/dir/list.m3u8" => Answer::ok("#EXTM3U\n#EXTINF:4,\ns.ts\n"),
        "/real/dir/s.ts" => Answer::ok("DATA"),
        _ => Answer::status(404),
    });
    let proxy = proxy();
    let piece = first_piece(&proxy, &origin, "/go.m3u8");
    assert_eq!(get(&piece).body, b"DATA");
}

#[test]
fn a_link_labelled_hls_that_is_a_plain_file_is_served_as_the_file_it_is() {
    let movie: Vec<u8> = b"\x00\x00\x00\x18ftypmp42-and-then-some-video-bytes".to_vec();
    let served = movie.clone();
    let origin = FakeOrigin::start(move |request| match request.header("range") {
        Some("bytes=4-7") => Answer::status(206)
            .with("Content-Type", "video/mp4")
            .tap_body(&served[4..8]),
        _ => Answer::ok(served.clone()).with("Content-Type", "video/mp4"),
    });
    let proxy = proxy();
    let url = proxy
        .open(&StreamSpec {
            url: origin.url("/movie.m3u8"),
            ..StreamSpec::default()
        })
        .expect("opens");
    let whole = get(&url);
    assert_eq!(
        (whole.status, whole.body.as_slice()),
        (200, movie.as_slice())
    );
    assert_eq!(whole.header("content-type"), Some("video/mp4"));
    let part = request("GET", &url, &[("Range", "bytes=4-7")]).expect("answers");
    assert_eq!((part.status, part.body.as_slice()), (206, &movie[4..8]));
}

#[test]
fn head_gets_the_head_of_the_rewritten_playlist_and_no_body() {
    let origin = FakeOrigin::start(show);
    let proxy = proxy();
    let url = proxy
        .open(&spec(origin.url("/show/master.m3u8")))
        .expect("opens");
    let head = request("HEAD", &url, &[]).expect("answers");
    assert_eq!(head.status, 200);
    assert!(head.body.is_empty());
    let length: usize = head
        .header("content-length")
        .expect("a length")
        .parse()
        .expect("a number");
    assert_eq!(length, get(&url).body.len());
}
