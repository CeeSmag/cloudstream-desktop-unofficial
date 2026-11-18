//! T2: the real poster downloader against a small local web server.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use poster_artwork::{
    decode_artwork, ArtworkError, ArtworkFetcher, UreqArtworkFetcher, MAX_IMAGE_BYTES,
};

/// Serves `/poster.png`, `/missing`, `/huge`, `/hang` and `/guarded` until dropped.
struct Server {
    address: String,
    stop: Arc<AtomicBool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(800, 1200, image::Rgba([9, 9, 9, 255]));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("encode");
    bytes
}

fn read_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&head).into_owned()
}

fn respond(stream: &mut TcpStream, status: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn serve(mut stream: TcpStream) {
    let head = read_head(&mut stream);
    let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
    match path.as_str() {
        "/poster.png" => respond(&mut stream, "200 OK", &png()),
        "/huge" => respond(
            &mut stream,
            "200 OK",
            &vec![0u8; (MAX_IMAGE_BYTES + 10) as usize],
        ),
        "/hang" => std::thread::sleep(Duration::from_secs(3)),
        "/guarded"
            if head
                .to_ascii_lowercase()
                .contains("referer: https://site.test/") =>
        {
            respond(&mut stream, "200 OK", &png());
        }
        "/guarded" => respond(&mut stream, "403 Forbidden", b""),
        _ => respond(&mut stream, "404 Not Found", b""),
    }
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("non blocking");
        let address = format!("http://{}", listener.local_addr().expect("address"));
        let stop = Arc::new(AtomicBool::new(false));
        let halt = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !halt.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).expect("blocking");
                        std::thread::spawn(move || serve(stream));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        });
        Self { address, stop }
    }
}

#[test]
fn fetch_good_poster_downloads_and_decodes_shrunk() {
    let server = Server::start();
    let bytes = UreqArtworkFetcher::default()
        .fetch(&format!("{}/poster.png", server.address), &[])
        .expect("download");
    let decoded = decode_artwork(&bytes).expect("decode");
    assert_eq!((decoded.width, decoded.height), (360, 540));
}

#[test]
fn fetch_missing_poster_reports_status() {
    let server = Server::start();
    let outcome = UreqArtworkFetcher::default().fetch(&format!("{}/missing", server.address), &[]);
    assert_eq!(outcome, Err(ArtworkError::Status(404)));
}

#[test]
fn fetch_oversized_body_is_refused() {
    let server = Server::start();
    let outcome = UreqArtworkFetcher::default().fetch(&format!("{}/huge", server.address), &[]);
    assert_eq!(outcome, Err(ArtworkError::TooLarge(MAX_IMAGE_BYTES)));
}

#[test]
fn fetch_silent_host_times_out() {
    let server = Server::start();
    let fetcher = UreqArtworkFetcher::with_timeout(Duration::from_millis(300));
    let outcome = fetcher.fetch(&format!("{}/hang", server.address), &[]);
    assert!(
        matches!(outcome, Err(ArtworkError::Network(_))),
        "{outcome:?}"
    );
}

#[test]
fn fetch_sends_given_headers() {
    let server = Server::start();
    let fetcher = UreqArtworkFetcher::default();
    let url = format!("{}/guarded", server.address);
    assert_eq!(fetcher.fetch(&url, &[]), Err(ArtworkError::Status(403)));
    let referer = [("Referer".to_owned(), "https://site.test/".to_owned())];
    assert!(fetcher.fetch(&url, &referer).is_ok());
}
