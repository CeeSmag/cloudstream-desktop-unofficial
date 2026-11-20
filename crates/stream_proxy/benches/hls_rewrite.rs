//! Hot-path benchmark for opening and rewriting an HLS playlist through `StreamProxy`.
#![allow(missing_docs)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use stream_proxy::{ProxyConfig, StreamProxy, StreamSpec};

fn build_playlist(segments: usize) -> String {
    let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n");
    for i in 0..segments {
        out.push_str(&format!("#EXTINF:6.0,\nseg_{i:04}.ts\n"));
    }
    out.push_str("#EXT-X-ENDLIST\n");
    out
}

fn start_origin(body: String) -> Option<(String, Arc<AtomicBool>, thread::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let addr = listener.local_addr().ok()?;
    let _ = listener.set_nonblocking(true);
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                    let mut buf = [0u8; 512];
                    let _ = stream.read(&mut buf);
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apple.mpegurl\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes());
                }
                Err(_) => thread::sleep(Duration::from_millis(2)),
            }
        }
    });
    Some((format!("http://{addr}/index.m3u8"), stop, handle))
}

fn bench_hls_proxy(c: &mut Criterion) {
    let Some((origin_url, stop, handle)) = start_origin(build_playlist(120)) else {
        return;
    };
    let Ok(proxy) = StreamProxy::start(ProxyConfig::default()) else {
        stop.store(true, Ordering::Relaxed);
        let _ = handle.join();
        return;
    };
    let spec = StreamSpec {
        url: origin_url,
        ..StreamSpec::default()
    };

    c.bench_function("stream_proxy_open_and_rewrite_120_segments", |b| {
        b.iter(|| {
            if let Ok(url) = proxy.open(black_box(&spec)) {
                if let Ok(mut resp) = ureq::get(&url).call() {
                    let _ = resp.body_mut().read_to_string();
                }
            }
        })
    });

    proxy.shutdown();
    stop.store(true, Ordering::Relaxed);
    let _ = handle.join();
}

criterion_group!(benches, bench_hls_proxy);
criterion_main!(benches);
