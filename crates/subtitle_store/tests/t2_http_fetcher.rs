//! T2: the real HTTP downloader against a small local web server: a good answer, a missing file,
//! a host that sends the headers it was asked to, one that never answers, and a flood of requests.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use subtitle_store::{FetchError, SubtitleFetcher, UreqSubtitleFetcher};

/// Serves `/ok`, `/missing`, `/hang` and `/echo-referer` until dropped.
struct Server {
    address: String,
    stop: Arc<AtomicBool>,
    referers: Arc<Mutex<Vec<String>>>,
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => request.push(byte[0]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let answer = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/vtt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(answer.as_bytes());
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("non blocking");
        let address = format!("http://{}", listener.local_addr().expect("address"));
        let (stop, referers) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(Mutex::new(Vec::new())),
        );
        let (halt, seen) = (Arc::clone(&stop), Arc::clone(&referers));
        std::thread::spawn(move || {
            while !halt.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let seen = Arc::clone(&seen);
                        std::thread::spawn(move || serve(stream, &seen));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        });
        Self {
            address,
            stop,
            referers,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.address)
    }
}

fn serve(mut stream: TcpStream, referers: &Mutex<Vec<String>>) {
    stream.set_nonblocking(false).expect("blocking");
    let request = read_request(&mut stream);
    let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
    if let Some(referer) = request
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("referer:"))
    {
        referers.lock().expect("lock").push(referer.to_owned());
    }
    match path.as_str() {
        "/ok" => respond(
            &mut stream,
            "200 OK",
            "WEBVTT\n\n00:01.000 --> 00:02.000\nhi\n",
        ),
        "/busy" => respond(&mut stream, "503 Service Unavailable", "busy"),
        "/hang" => std::thread::sleep(Duration::from_secs(5)),
        _ => respond(&mut stream, "404 Not Found", "missing"),
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn fetcher() -> UreqSubtitleFetcher {
    UreqSubtitleFetcher::with_timeout(Duration::from_millis(600))
}

#[test]
fn a_subtitle_is_downloaded() {
    let server = Server::start();
    let body = fetcher().fetch(&server.url("/ok"), &[]).expect("downloads");
    assert!(String::from_utf8_lossy(&body).starts_with("WEBVTT"));
}

#[test]
fn error_statuses_are_reported_and_only_busy_ones_are_worth_retrying() {
    let server = Server::start();
    let missing = fetcher()
        .fetch(&server.url("/missing"), &[])
        .expect_err("404");
    assert_eq!(missing, FetchError::Status { status: 404 });
    assert!(!missing.is_transient());
    let busy = fetcher().fetch(&server.url("/busy"), &[]).expect_err("503");
    assert_eq!(busy, FetchError::Status { status: 503 });
    assert!(busy.is_transient());
}

#[test]
fn a_host_that_never_answers_is_given_up_on_at_the_timeout() {
    let server = Server::start();
    let started = Instant::now();
    let error = fetcher()
        .fetch(&server.url("/hang"), &[])
        .expect_err("times out");
    assert!(matches!(error, FetchError::Network { .. }), "{error:?}");
    assert!(error.is_transient());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "waited {:?}",
        started.elapsed()
    );
}

#[test]
fn a_host_that_is_not_there_is_a_network_failure() {
    let error = fetcher()
        .fetch("http://127.0.0.1:9/nothing", &[])
        .expect_err("refused");
    assert!(matches!(error, FetchError::Network { .. }), "{error:?}");
}

#[test]
fn the_headers_the_provider_asked_for_are_sent() {
    let server = Server::start();
    let headers = vec![("Referer".to_owned(), "https://provider.test/".to_owned())];
    fetcher()
        .fetch(&server.url("/ok"), &headers)
        .expect("downloads");
    let seen = server.referers.lock().expect("lock");
    assert_eq!(seen.len(), 1);
    assert!(seen[0].contains("https://provider.test/"));
}

/// Many downloads at once, as the app does: every one is answered, none is mixed up with another.
#[test]
fn many_downloads_at_once_all_get_their_own_answer() {
    let server = Server::start();
    let fetcher = Arc::new(fetcher());
    let handles: Vec<_> = (0..16)
        .map(|n| {
            let (fetcher, url) = (
                Arc::clone(&fetcher),
                server.url(if n % 4 == 0 { "/missing" } else { "/ok" }),
            );
            std::thread::spawn(move || (n, fetcher.fetch(&url, &[])))
        })
        .collect();
    for handle in handles {
        let (n, outcome) = handle.join().expect("thread");
        assert_eq!(
            outcome.is_ok(),
            n % 4 != 0,
            "download {n} got the wrong answer: {outcome:?}"
        );
    }
}
