//! Race tests (RULES section 9) for the running proxy, against real sockets.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::fake_origin::{get, request, Answer, FakeOrigin};
use crate::http_request::HttpRequest;

/// A deadlock must fail a test, not hang the run.
const TEST_LIMIT: Duration = Duration::from_secs(30);

const PLAYERS: usize = 8;
const ROUNDS: usize = 40;
const PIECE_BYTES: usize = 200_000;

fn proxy_with(config: ProxyConfig) -> StreamProxy {
    StreamProxy::start_with(config, Arc::new(UreqUpstream::without_proxy())).expect("starts")
}

/// Runs `body` on its own thread and fails when it has not finished within [`TEST_LIMIT`].
fn finishes_within_limit<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(body());
    });
    finished
        .recv_timeout(TEST_LIMIT)
        .expect("the test body did not finish: a deadlock, or a panic in a worker")
}

fn first_media_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .expect("a piece")
        .to_owned()
}

fn piece_of(proxy: &StreamProxy, playlist_url: String) -> String {
    let spec = StreamSpec {
        url: playlist_url,
        ..StreamSpec::default()
    };
    first_media_line(&get(&proxy.open(&spec).expect("opens")).text())
}

/// Two streams whose pieces are told apart by their content.
fn two_streams(request: &HttpRequest) -> Answer {
    match request.target.as_str() {
        "/a/p.m3u8" => Answer::ok("#EXTM3U\n#EXTINF:4,\nseg.ts\n"),
        "/b/p.m3u8" => Answer::ok("#EXTM3U\n#EXTINF:4,\nseg.ts\n"),
        "/a/seg.ts" => Answer::ok(vec![b'A'; PIECE_BYTES]),
        "/b/seg.ts" => Answer::ok(vec![b'B'; PIECE_BYTES]),
        _ => Answer::status(404),
    }
}

#[test]
fn many_players_reading_two_streams_at_once_always_get_the_piece_they_asked_for() {
    finishes_within_limit(|| {
        let origin = FakeOrigin::start(two_streams);
        let proxy = proxy_with(ProxyConfig::default());
        let piece_a = piece_of(&proxy, origin.url("/a/p.m3u8"));
        let piece_b = piece_of(&proxy, origin.url("/b/p.m3u8"));
        let players: Vec<_> = (0..PLAYERS)
            .map(|player| {
                let (a, b) = (piece_a.clone(), piece_b.clone());
                std::thread::spawn(move || {
                    for round in 0..ROUNDS {
                        let (url, letter) = if (player + round) % 2 == 0 {
                            (&a, b'A')
                        } else {
                            (&b, b'B')
                        };
                        let reply = request("GET", url, &[]).expect("answers");
                        assert_eq!(reply.status, 200);
                        assert_eq!(reply.body.len(), PIECE_BYTES, "a whole piece");
                        assert!(
                            reply.body.iter().all(|byte| *byte == letter),
                            "a piece of the other stream"
                        );
                    }
                })
            })
            .collect();
        for player in players {
            player.join().expect("a player finished");
        }
        assert_eq!(
            proxy.open_connections(),
            0,
            "every connection was given back"
        );
    });
}

/// A player that keeps asking for the piece until told to stop. An answer that was cut short or
/// refused because of the shutdown is fine; one that claims to be complete must be right.
fn read_until(closing: &AtomicBool, piece: &str) {
    while !closing.load(Ordering::SeqCst) {
        let Ok(reply) = request("GET", piece, &[]) else {
            continue;
        };
        let whole = reply.body.len() == PIECE_BYTES;
        assert!(reply.status != 200 || !whole || reply.body.iter().all(|byte| *byte == b'A'));
    }
}

#[test]
fn shutting_down_while_players_read_ends_every_connection_and_no_complete_answer_is_wrong() {
    finishes_within_limit(|| {
        let origin = FakeOrigin::start(two_streams);
        let proxy = proxy_with(ProxyConfig::default());
        let piece = piece_of(&proxy, origin.url("/a/p.m3u8"));
        let closing = Arc::new(AtomicBool::new(false));
        let players: Vec<_> = (0..PLAYERS)
            .map(|_| {
                let (piece, closing) = (piece.clone(), Arc::clone(&closing));
                std::thread::spawn(move || read_until(&closing, &piece))
            })
            .collect();
        std::thread::sleep(Duration::from_millis(50));
        closing.store(true, Ordering::SeqCst);
        let started = Instant::now();
        proxy.shutdown();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "shutdown must not hang"
        );
        for player in players {
            player.join().expect("a player finished");
        }
        let again = StreamSpec {
            url: origin.url("/a/p.m3u8"),
            ..StreamSpec::default()
        };
        assert!(matches!(proxy.open(&again), Err(ProxyError::ShutDown)));
        assert_eq!(proxy.open_connections(), 0, "no connection was left behind");
    });
}

/// An origin whose first request for `/slow.ts` waits until the test lets it go.
struct SlowOrigin {
    origin: FakeOrigin,
    arrived: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

fn slow_origin() -> SlowOrigin {
    let (arrived_tx, arrived) = mpsc::channel();
    let (release, release_rx) = mpsc::channel::<()>();
    let (arrived_tx, release_rx) = (Mutex::new(arrived_tx), Mutex::new(release_rx));
    let first = AtomicBool::new(true);
    let origin = FakeOrigin::start(move |request| match request.target.as_str() {
        "/p.m3u8" => Answer::ok("#EXTM3U\n#EXTINF:4,\nslow.ts\n"),
        "/slow.ts" => {
            if first.swap(false, Ordering::SeqCst) {
                let _ = arrived_tx.lock().expect("lock").send(());
                let _ = release_rx
                    .lock()
                    .expect("lock")
                    .recv_timeout(Duration::from_secs(20));
            }
            Answer::ok("SLOW-BODY")
        }
        _ => Answer::status(404),
    });
    SlowOrigin {
        origin,
        arrived,
        release,
    }
}

#[test]
fn a_busy_proxy_refuses_with_503_and_serves_again_as_soon_as_a_place_is_free() {
    finishes_within_limit(|| {
        let slow = slow_origin();
        let config = ProxyConfig {
            max_connections: 1,
            ..ProxyConfig::default()
        };
        let proxy = proxy_with(config);
        let piece = piece_of(&proxy, slow.origin.url("/p.m3u8"));
        let stuck = {
            let piece = piece.clone();
            std::thread::spawn(move || get(&piece))
        };
        slow.arrived
            .recv_timeout(TEST_LIMIT)
            .expect("the origin was asked");
        assert_eq!(get(&piece).status, 503, "the only place is taken");
        slow.release.send(()).expect("released");
        assert_eq!(stuck.join().expect("finished").body, b"SLOW-BODY");
        assert_eq!(
            get(&piece).status,
            200,
            "the place was given back before the answer ended"
        );
    });
}

/// The refusal is read only after the proxy has closed the connection: had the proxy closed it
/// with the request unread, TCP would have reset it, and on Windows a reset destroys the 503
/// before it is read (Linux keeps it; the next test covers the same rule on every system).
#[test]
fn a_busy_refusal_is_still_there_to_read_after_the_proxy_has_closed_the_connection() {
    use std::io::{Read, Write};
    finishes_within_limit(|| {
        let slow = slow_origin();
        let proxy = proxy_with(ProxyConfig {
            max_connections: 1,
            ..ProxyConfig::default()
        });
        let piece = piece_of(&proxy, slow.origin.url("/p.m3u8"));
        let stuck = {
            let piece = piece.clone();
            std::thread::spawn(move || get(&piece))
        };
        slow.arrived
            .recv_timeout(TEST_LIMIT)
            .expect("the origin was asked");
        let mut refused =
            std::net::TcpStream::connect(("127.0.0.1", proxy.port())).expect("connects");
        let host = format!("127.0.0.1:{}", proxy.port());
        let path = piece
            .split_once(host.as_str())
            .map_or("/", |(_, path)| path);
        write!(refused, "GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n").expect("sent");
        // Long enough for the proxy to answer and close; a slow proxy only makes this pass late.
        std::thread::sleep(Duration::from_millis(300));
        let mut answer = Vec::new();
        let read = refused.read_to_end(&mut answer);
        assert!(read.is_ok(), "the refusal was destroyed: {read:?}");
        assert!(
            answer.starts_with(b"HTTP/1.1 503"),
            "{}",
            String::from_utf8_lossy(&answer)
        );
        slow.release.send(()).expect("released");
        stuck.join().expect("finished");
    });
}

/// A busy proxy reads the player's request before it refuses (see `refuse_busy`): it answers only
/// once the request is complete or its short wait for it is over, never before.
#[test]
fn a_busy_refusal_waits_for_the_request_before_answering() {
    use std::io::{Read, Write};
    finishes_within_limit(|| {
        let slow = slow_origin();
        let proxy = proxy_with(ProxyConfig {
            max_connections: 1,
            ..ProxyConfig::default()
        });
        let piece = piece_of(&proxy, slow.origin.url("/p.m3u8"));
        let stuck = {
            let piece = piece.clone();
            std::thread::spawn(move || get(&piece))
        };
        slow.arrived
            .recv_timeout(TEST_LIMIT)
            .expect("the origin was asked");
        // Timed from before the connection exists, so the proxy's wait cannot have started first.
        let started = Instant::now();
        let mut refused =
            std::net::TcpStream::connect(("127.0.0.1", proxy.port())).expect("connects");
        refused
            .write_all(b"GET /half HTTP/1.1\r\nHo")
            .expect("half sent");
        let mut answer = Vec::new();
        let _ = refused.read_to_end(&mut answer);
        let waited = started.elapsed();
        assert!(answer.starts_with(b"HTTP/1.1 503"), "{answer:?}");
        assert!(
            waited >= crate::proxy_server::BUSY_READ_TIMEOUT,
            "refused after {waited:?}, before the rest of the request could arrive"
        );
        slow.release.send(()).expect("released");
        stuck.join().expect("finished");
    });
}

#[test]
fn shutdown_waits_for_a_stuck_connection_only_as_long_as_configured() {
    finishes_within_limit(|| {
        let slow = slow_origin();
        let config = ProxyConfig {
            shutdown_wait: Duration::from_millis(100),
            ..ProxyConfig::default()
        };
        let proxy = proxy_with(config);
        let piece = piece_of(&proxy, slow.origin.url("/p.m3u8"));
        let stuck = std::thread::spawn(move || request("GET", &piece, &[]));
        slow.arrived
            .recv_timeout(TEST_LIMIT)
            .expect("the origin was asked");
        let started = Instant::now();
        proxy.shutdown();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "shutdown gave up waiting"
        );
        slow.release.send(()).expect("released");
        let _ = stuck.join().expect("the stuck player finished");
        let deadline = Instant::now() + Duration::from_secs(10);
        while proxy.open_connections() > 0 && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(
            proxy.open_connections(),
            0,
            "the stuck connection ended once the origin answered"
        );
    });
}
