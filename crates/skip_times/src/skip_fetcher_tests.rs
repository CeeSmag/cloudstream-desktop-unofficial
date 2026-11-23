//! T2: the real HTTP fetcher against a one-answer server on a local socket.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread::JoinHandle;

use super::*;

/// Answers one request with `status` and `body`; hands the request's head back when joined.
struct OneAnswer {
    url: String,
    worker: JoinHandle<String>,
}

impl OneAnswer {
    fn start(status: &str, body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/skip", listener.local_addr().expect("address"));
        let status = status.to_owned();
        let (ready, started) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = ready.send(());
            let (stream, _) = listener.accept().expect("a request");
            let mut reader = BufReader::new(stream);
            let mut head = String::new();
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                head.push_str(&line);
                line.clear();
            }
            let answer = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = reader.get_mut().write_all(answer.as_bytes());
            head
        });
        started.recv().expect("server thread runs");
        Self { url, worker }
    }

    /// The request head the server read.
    fn request(self) -> String {
        self.worker.join().expect("server thread")
    }
}

#[test]
fn a_good_answer_is_returned_whole_and_names_the_app() {
    // Bigger than a few hundred bytes, as a long list of segments can be, yet within the limit.
    let body = "s".repeat(4_000);
    let server = OneAnswer::start("200 OK", body.clone());
    let text = UreqSkipFetcher::default()
        .get_text(&server.url)
        .expect("answered");
    assert_eq!(text, body);
    let head = server.request().to_ascii_lowercase();
    assert!(head.contains("user-agent: cloudstream-rs"), "{head}");
}

#[test]
fn an_error_status_is_reported_with_its_code() {
    let server = OneAnswer::start("404 Not Found", "nobody knows this title".into());
    let result = UreqSkipFetcher::default().get_text(&server.url);
    assert_eq!(result, Err(SkipError::Status { status: 404 }));
    let _ = server.request();
}

#[test]
fn an_answer_bigger_than_the_limit_is_cut_at_the_limit() {
    let limit = usize::try_from(MAX_BYTES).expect("fits");
    let server = OneAnswer::start("200 OK", "x".repeat(limit + 10_000));
    let text = UreqSkipFetcher::default()
        .get_text(&server.url)
        .expect("answered");
    assert_eq!(text.len(), limit);
    let _ = server.request();
}

#[test]
fn a_host_that_is_not_there_is_a_network_error() {
    // Bound and closed again: nothing listens on the port.
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port")
        .port();
    let result = UreqSkipFetcher::default().get_text(&format!("http://127.0.0.1:{port}/skip"));
    assert!(
        matches!(result, Err(SkipError::Network { .. })),
        "{result:?}"
    );
}
