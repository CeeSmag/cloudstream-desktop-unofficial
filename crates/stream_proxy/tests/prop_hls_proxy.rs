//! Property-based tests for `stream_proxy` URL validation and HLS playlist rewriting via local HTTP.
#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use proptest::prelude::*;
use stream_proxy::{ProxyConfig, StreamProxy, StreamSpec};

struct PlaylistOrigin {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl PlaylistOrigin {
    fn start(playlist_body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut s, _)) => {
                        let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = [0u8; 1024];
                        let _ = s.read(&mut buf);
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apple.mpegurl\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            playlist_body.len(),
                            playlist_body
                        );
                        let _ = s.write_all(resp.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(3));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for PlaylistOrigin {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn stream_proxy_rejects_non_http_schemes(
        scheme in prop_oneof![Just("file"), Just("ftp"), Just("javascript"), Just("data")],
        rest in "[a-zA-Z0-9/_.-]{1,30}",
    ) {
        let proxy = StreamProxy::start(ProxyConfig::default()).expect("start proxy");
        let spec = StreamSpec {
            url: format!("{scheme}://{rest}"),
            ..StreamSpec::default()
        };
        prop_assert!(proxy.open(&spec).is_err());
        proxy.shutdown();
    }

    #[test]
    fn stream_proxy_rewrites_every_media_segment_to_loopback_proxy(
        seg_names in prop::collection::vec("[a-z0-9]{3,10}", 1..8)
    ) {
        let mut playlist = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n");
        for name in &seg_names {
            playlist.push_str(&format!("#EXTINF:6.0,\n{name}.ts\n"));
        }
        playlist.push_str("#EXT-X-ENDLIST\n");

        let origin = PlaylistOrigin::start(playlist);
        let proxy = StreamProxy::start(ProxyConfig::default()).expect("start proxy");
        let proxied = proxy
            .open(&StreamSpec {
                url: format!("{}/index.m3u8", origin.base_url),
                ..StreamSpec::default()
            })
            .expect("open");

        let resp = ureq::get(&proxied)
            .call()
            .expect("fetch rewritten playlist")
            .body_mut()
            .read_to_string()
            .expect("read body");

        let expected_prefix = format!("http://127.0.0.1:{}/", proxy.port());
        let rewritten_urls: Vec<&str> = resp
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        prop_assert_eq!(rewritten_urls.len(), seg_names.len());
        for url in rewritten_urls {
            prop_assert!(url.starts_with(&expected_prefix), "not proxied: {url}");
        }
        proxy.shutdown();
    }
}
