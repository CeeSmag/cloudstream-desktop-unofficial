//! T2 tests for [`UreqSetupFetcher`] against a small HTTP server on a local socket (no internet).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use super::*;

const SETUP: &[u8] = b"MZ a pretend setup";
const LIMIT: u64 = 256 * 1024;

fn loopback(url: &str) -> bool {
    url.starts_with("http://127.0.0.1:")
}

/// Answers by path; every answer closes the connection.
fn respond(stream: &mut TcpStream, path: &str) -> std::io::Result<()> {
    let big = vec![b'x'; 300 * 1024];
    let exact = vec![b'y'; usize::try_from(LIMIT).unwrap()];
    let (head, body): (String, &[u8]) = match path {
        "/setup" => (format!("200 OK\r\nContent-Length: {}", SETUP.len()), SETUP),
        "/hop" => (
            "302 Found\r\nLocation: /setup\r\nContent-Length: 0".into(),
            b"",
        ),
        "/away" => (
            "302 Found\r\nLocation: http://example.invalid/setup.exe\r\nContent-Length: 0".into(),
            b"",
        ),
        "/relative" => (
            "302 Found\r\nLocation: setup\r\nContent-Length: 0".into(),
            b"",
        ),
        "/loop" => (
            "302 Found\r\nLocation: /loop\r\nContent-Length: 0".into(),
            b"",
        ),
        "/nowhere" => ("302 Found\r\nContent-Length: 0".into(), b""),
        "/declared-big" => (format!("200 OK\r\nContent-Length: {}", LIMIT + 1), b""),
        "/exact" => (format!("200 OK\r\nContent-Length: {LIMIT}"), &exact),
        "/big" | "/slow-big" => ("200 OK".into(), &big),
        _ => ("404 Not Found\r\nContent-Length: 0".into(), b""),
    };
    write!(stream, "HTTP/1.1 {head}\r\nConnection: close\r\n\r\n")?;
    stream.write_all(body)
}

fn serve(stream: TcpStream) {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut line = String::new();
    while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
        line.clear();
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    let _ = respond(reader.get_mut(), &path);
}

/// A server on a free local port; it lives until the test process ends.
fn server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || serve(stream));
        }
    });
    address
}

struct Download {
    result: Result<(), SetupError>,
    dest: PathBuf,
    progress: Vec<(u64, u64)>,
}

fn download(name: &str, path: &str, cancel: &CancelFlag) -> Download {
    let dir = std::env::temp_dir().join(format!("cs_wv2_http_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dest = dir.join("setup.exe");
    let url = format!("{}{path}", server());
    let mut progress = Vec::new();
    let request = FetchRequest {
        url: &url,
        dest: &dest,
        cancel,
    };
    let fetcher = UreqSetupFetcher::with_rule(loopback, LIMIT);
    let result = fetcher.fetch(request, &mut |done, total| progress.push((done, total)));
    Download {
        result,
        dest,
        progress,
    }
}

impl Download {
    /// Neither the file nor its `.part` may be left after a failure.
    fn assert_nothing_written(&self) {
        assert!(!self.dest.exists(), "{}", self.dest.display());
        assert!(!partial_path(&self.dest).exists());
    }
}

#[test]
fn a_setup_is_downloaded_with_its_progress() {
    let got = download("ok", "/setup", &CancelFlag::new());
    assert_eq!(got.result, Ok(()));
    assert_eq!(std::fs::read(&got.dest).unwrap(), SETUP);
    assert!(!partial_path(&got.dest).exists());
    let size = SETUP.len() as u64;
    assert_eq!(got.progress.last(), Some(&(size, size)));
}

#[test]
fn allowed_redirects_are_followed() {
    let got = download("hop", "/hop", &CancelFlag::new());
    assert_eq!(got.result, Ok(()));
    assert_eq!(std::fs::read(&got.dest).unwrap(), SETUP);
}

#[test]
fn a_redirect_to_another_host_is_refused() {
    let got = download("away", "/away", &CancelFlag::new());
    assert_eq!(
        got.result,
        Err(SetupError::WrongHost("example.invalid".into()))
    );
    got.assert_nothing_written();
}

#[test]
fn the_first_address_is_checked_too() {
    let dest = std::env::temp_dir().join(format!("cs_wv2_http_first_{}.exe", std::process::id()));
    let cancel = CancelFlag::new();
    let request = FetchRequest {
        url: "https://example.org/setup.exe",
        dest: &dest,
        cancel: &cancel,
    };
    let result = UreqSetupFetcher::microsoft().fetch(request, &mut |_, _| {});
    assert_eq!(result, Err(SetupError::WrongHost("example.org".into())));
}

#[test]
fn unusable_redirects_and_answers_fail() {
    for (path, expected) in [
        ("/relative", "a redirect that cannot be followed"),
        ("/loop", "too many redirects"),
        ("/nowhere", "a redirect without an address"),
        ("/gone", "the server answered HTTP 404"),
    ] {
        let got = download(&path.replace('/', ""), path, &CancelFlag::new());
        assert_eq!(
            got.result,
            Err(SetupError::Download(expected.into())),
            "{path}"
        );
        got.assert_nothing_written();
    }
}

#[test]
fn a_setup_declared_too_large_is_not_read() {
    let got = download("declared", "/declared-big", &CancelFlag::new());
    assert_eq!(got.result, Err(SetupError::TooLarge { limit: LIMIT }));
    assert!(got.progress.is_empty());
    got.assert_nothing_written();
}

#[test]
fn a_setup_growing_too_large_is_stopped_while_reading() {
    let got = download("growing", "/big", &CancelFlag::new());
    assert_eq!(got.result, Err(SetupError::TooLarge { limit: LIMIT }));
    assert!(got.progress.iter().all(|(done, _)| *done <= LIMIT));
    got.assert_nothing_written();
}

#[test]
fn a_setup_exactly_at_the_cap_is_accepted() {
    let got = download("exact", "/exact", &CancelFlag::new());
    assert_eq!(got.result, Ok(()));
    assert_eq!(std::fs::metadata(&got.dest).unwrap().len(), LIMIT);
}

/// The copy itself, from memory: chunks are 64 KiB, a body exactly at the limit passes and one
/// byte more does not.
#[test]
fn the_copy_reads_in_64_kib_chunks_up_to_the_limit() {
    let dir = std::env::temp_dir().join(format!("cs_wv2_copy_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dest = dir.join("copy.part");
    let cancel = CancelFlag::new();
    let body = vec![7_u8; 200 * 1024];
    let size = body.len() as u64;
    let copy = Copy {
        limit: size,
        total: 0,
        cancel: &cancel,
    };
    let mut reports = Vec::new();
    let result = copy.run(&mut body.as_slice(), &dest, &mut |done, _| {
        reports.push(done)
    });
    assert_eq!(result, Ok(()));
    assert_eq!(reports.first(), Some(&(64 * 1024)));
    assert_eq!(reports.last(), Some(&size));
    let over = Copy {
        limit: size - 1,
        total: 0,
        cancel: &cancel,
    };
    let result = over.run(&mut body.as_slice(), &dest, &mut |_, _| {});
    assert_eq!(result, Err(SetupError::TooLarge { limit: size - 1 }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cancelled_download_stops_and_leaves_nothing() {
    let cancel = CancelFlag::new();
    cancel.cancel();
    let got = download("cancelled", "/setup", &cancel);
    assert_eq!(got.result, Err(SetupError::Cancelled));
    got.assert_nothing_written();
}

#[test]
fn a_cancel_mid_download_stops_after_the_current_chunk() {
    let dir = std::env::temp_dir().join(format!("cs_wv2_http_mid_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dest = dir.join("setup.exe");
    let url = format!("{}/slow-big", server());
    let cancel = CancelFlag::new();
    let request = FetchRequest {
        url: &url,
        dest: &dest,
        cancel: &cancel,
    };
    let fetcher = UreqSetupFetcher::with_rule(loopback, 1024 * 1024);
    let mut chunks = 0;
    let result = fetcher.fetch(request, &mut |_, _| {
        chunks += 1;
        cancel.cancel();
    });
    assert_eq!(result, Err(SetupError::Cancelled));
    assert_eq!(chunks, 1);
    assert!(!dest.exists() && !partial_path(&dest).exists());
}
