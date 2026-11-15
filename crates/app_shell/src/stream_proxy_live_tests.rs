//! T4: an HLS stream is played by the real libmpv, directly and through the real proxy.
//!
//! The origin is a small web server on this PC. It hands out a one-segment playlist whose segment is
//! called `clip.jpg` (providers hide segments behind picture names), answers 403 to any request that
//! does not carry the provider's headers, and answers 403 to any request with a `Range` header:
//! some hosts do, and libmpv sends `Range: bytes=0-` on every request, so the stream cannot be
//! played directly.
//!
//! Needs libmpv and any MPEG-TS file of a few seconds (`CS_TEST_TS_SEGMENT`):
//! `CS_LIBMPV_PATH=...\libmpv-2.dll CS_TEST_TS_SEGMENT=...\clip.ts cargo test -p app_shell --lib -- --ignored hls`

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use app_services::RoutedPlayer;
use player_api::{PlaybackRequest, PlaybackState, StreamKind, VideoPlayer};
use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};

use crate::stream_proxy_router::StreamProxyRouter;

const PLAYLIST: &str = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:8\n\
#EXT-X-PLAYLIST-TYPE:VOD\n#EXTINF:7.0,\nclip.jpg\n#EXT-X-ENDLIST\n";

/// How far a stream must have played before it counts as playing.
const PLAYED_MS: i64 = 1_500;

/// What the origin was asked.
#[derive(Debug, Clone)]
struct Asked {
    path: String,
    /// The provider's headers came with the request.
    had_headers: bool,
    /// The request asked for a range.
    ranged: bool,
}

type Seen = Arc<Mutex<Vec<Asked>>>;

fn answer(mut stream: TcpStream, segment: &[u8], seen: &Mutex<Vec<Asked>>) {
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(count) => head.extend_from_slice(&chunk[..count]),
        }
    }
    let text = String::from_utf8_lossy(&head).to_ascii_lowercase();
    let path = text.split(' ').nth(1).unwrap_or_default().to_owned();
    let had_headers = text.contains("x-token: t") && text.contains("referer: https://site.test/");
    let ranged = text.contains(
        "
range:",
    );
    let allowed = had_headers && !ranged;
    seen.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Asked {
            path: path.clone(),
            had_headers,
            ranged,
        });
    let (status, kind, body) = match (allowed, path.as_str()) {
        (false, _) => ("403 Forbidden", "text/plain", b"no".to_vec()),
        (true, "/hls/master.m3u8") => ("200 OK", "text/plain", PLAYLIST.as_bytes().to_vec()),
        (true, "/hls/clip.jpg") => ("200 OK", "image/jpeg", segment.to_vec()),
        (true, _) => ("404 Not Found", "text/plain", b"none".to_vec()),
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

/// Starts the origin; returns the address of the playlist and what the origin is asked.
fn start_origin(segment: Vec<u8>) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
    let port = listener.local_addr().expect("an address").port();
    let seen: Seen = Arc::default();
    let (segment, recorded) = (Arc::new(segment), Arc::clone(&seen));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (segment, seen) = (Arc::clone(&segment), Arc::clone(&recorded));
            std::thread::spawn(move || answer(stream, &segment, &seen));
        }
    });
    (format!("http://127.0.0.1:{port}/hls/master.m3u8"), seen)
}

fn request(url: String) -> PlaybackRequest {
    PlaybackRequest {
        url,
        kind: StreamKind::Hls,
        title: "test".to_owned(),
        headers: vec![("X-Token".to_owned(), "t".to_owned())],
        referer: "https://site.test/".to_owned(),
        ..PlaybackRequest::default()
    }
}

/// Plays until the stream has played [`PLAYED_MS`], failed, or `limit` has passed.
fn play_and_watch(player: &dyn VideoPlayer, url: String, limit: Duration) -> PlaybackState {
    player.play(&request(url)).expect("loadfile accepted");
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        let status = player.status();
        let played = status.state == PlaybackState::Playing && status.position_ms >= PLAYED_MS;
        if played || matches!(status.state, PlaybackState::Failed(_)) {
            return status.state;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    player.status().state
}

fn headless_player() -> MpvPlayer {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("stream_proxy=debug,player_mpv=debug")
        .with_test_writer()
        .try_init();
    let library = locate_libmpv().expect("libmpv located");
    let options = PlayerOptions {
        headless: true,
        ..PlayerOptions::default()
    };
    MpvPlayer::new(&library, &options).expect("player starts")
}

fn segment() -> Vec<u8> {
    let path = std::env::var("CS_TEST_TS_SEGMENT").expect("CS_TEST_TS_SEGMENT names a .ts file");
    std::fs::read(path).expect("the segment can be read")
}

#[test]
#[ignore = "needs libmpv (CS_LIBMPV_PATH) and a TS file (CS_TEST_TS_SEGMENT)"]
fn hls_from_a_host_that_refuses_ranges_does_not_play_directly() {
    let (url, _) = start_origin(segment());
    let state = play_and_watch(&headless_player(), url, Duration::from_secs(20));
    assert!(
        matches!(state, PlaybackState::Failed(_)),
        "played directly, the stream was expected to fail, but it is {state:?}"
    );
}

#[test]
#[ignore = "needs libmpv (CS_LIBMPV_PATH) and a TS file (CS_TEST_TS_SEGMENT)"]
fn hls_from_a_host_that_refuses_ranges_plays_through_the_proxy() {
    let (url, seen) = start_origin(segment());
    let router = StreamProxyRouter::new(|| true);
    let player = RoutedPlayer::new(Arc::new(headless_player()), Arc::new(router));
    let state = play_and_watch(&player, url, Duration::from_secs(30));
    assert_eq!(state, PlaybackState::Playing, "the stream did not play");
    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert!(
        seen.iter().all(|asked| asked.had_headers),
        "a request lacked the headers: {seen:?}"
    );
    let got_the_segment = seen
        .iter()
        .any(|asked| asked.path == "/hls/clip.jpg" && !asked.ranged);
    assert!(
        got_the_segment,
        "the segment was never fetched without a range: {seen:?}"
    );
    player.stop().expect("stops");
}
