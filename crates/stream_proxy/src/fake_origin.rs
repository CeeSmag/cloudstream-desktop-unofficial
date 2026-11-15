//! Test support: a small web server that answers from a function and remembers every request,
//! and a plain HTTP client that talks to the proxy the way a player does.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::http_request::{read_request, HttpRequest};
use crate::http_response::write_head;

/// What the fake origin answers.
pub(crate) struct Answer {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl Answer {
    pub(crate) fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub(crate) fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            body: body.into(),
            ..Self::status(200)
        }
    }

    pub(crate) fn with(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

type Route = dyn Fn(&HttpRequest) -> Answer + Send + Sync;

pub(crate) struct FakeOrigin {
    port: u16,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeOrigin {
    pub(crate) fn start(route: impl Fn(&HttpRequest) -> Answer + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("binds");
        let port = listener.local_addr().expect("has an address").port();
        let route: Arc<Route> = Arc::new(route);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopping = Arc::new(AtomicBool::new(false));
        let (seen, stop) = (Arc::clone(&requests), Arc::clone(&stopping));
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let (route, seen) = (Arc::clone(&route), Arc::clone(&seen));
                std::thread::spawn(move || answer_one(stream, &*route, &seen));
            }
        });
        Self {
            port,
            requests,
            stopping,
            thread: Some(thread),
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    pub(crate) fn requests(&self) -> Vec<HttpRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for FakeOrigin {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn answer_one(mut stream: TcpStream, route: &Route, seen: &Mutex<Vec<HttpRequest>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let Ok(request) = read_request(&mut stream, Instant::now() + Duration::from_secs(5)) else {
        return;
    };
    seen.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(request.clone());
    let answer = route(&request);
    let mut headers: Vec<(&str, String)> = answer
        .headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    headers.push(("Content-Length", answer.body.len().to_string()));
    if write_head(&mut stream, answer.status, &headers).is_ok() && request.method != "HEAD" {
        let _ = stream.write_all(&answer.body);
    }
}

/// What a client got back.
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl Reply {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(own, _)| own.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Sends a request to an `http://127.0.0.1:port/path` address and reads the whole answer. A
/// `Host` header is added unless one is given.
pub(crate) fn request(method: &str, url: &str, extra: &[(&str, &str)]) -> std::io::Result<Reply> {
    let authority_and_path = url.strip_prefix("http://").expect("an http address");
    let (authority, path) = authority_and_path
        .split_once('/')
        .map_or((authority_and_path, "/".to_owned()), |(a, p)| {
            (a, format!("/{p}"))
        });
    let mut stream = TcpStream::connect(authority)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut text = format!("{method} {path} HTTP/1.1\r\n");
    if !extra
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"))
    {
        text.push_str(&format!("Host: {authority}\r\n"));
    }
    for (name, value) in extra {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str("\r\n");
    stream.write_all(text.as_bytes())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    Ok(parse_reply(&raw))
}

/// `GET` with no extra headers.
pub(crate) fn get(url: &str) -> Reply {
    request("GET", url, &[]).expect("the server answers")
}

fn parse_reply(raw: &[u8]) -> Reply {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a complete head");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .expect("a status line");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_owned(), value.trim().to_owned()))
        .collect();
    Reply {
        status,
        headers,
        body: raw[split + 4..].to_vec(),
    }
}
