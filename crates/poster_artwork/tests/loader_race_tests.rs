//! Concurrent stress and deduplication race tests for `poster_artwork::ArtworkLoader` (RULES.md §9).
#![allow(clippy::too_many_lines)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use poster_artwork::{
    ArtworkError, ArtworkFetcher, ArtworkHeaders, ArtworkLoader, ArtworkLoaderConfig,
    ArtworkResult, DiskCache,
};

/// Minimal valid 1x1 PNG image bytes.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

struct CountingFetcher {
    fetches: AtomicUsize,
}

impl ArtworkFetcher for CountingFetcher {
    fn fetch(&self, url: &str, _headers: &[(String, String)]) -> Result<Vec<u8>, ArtworkError> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        if url.contains("corrupt") {
            return Ok(b"not-a-valid-image".to_vec());
        }
        Ok(TINY_PNG.to_vec())
    }
}

#[test]
fn artwork_loader_delivers_exactly_one_answer_per_concurrent_request() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cs_art_race_{}_{}", std::process::id(), nanos));
    let disk = Some(DiskCache::new(dir.clone()));
    let fetcher = Arc::new(CountingFetcher {
        fetches: AtomicUsize::new(0),
    });
    let loader = Arc::new(ArtworkLoader::new(
        fetcher,
        ArtworkLoaderConfig {
            workers: 4,
            memory_budget_bytes: 2 * 1024 * 1024,
            disk,
        },
    ));

    let total_requests = 160usize;
    let done = Arc::new((Mutex::new((0usize, 0usize, 0usize)), Condvar::new()));
    let mut spawners = Vec::new();

    for t in 0..4usize {
        let loader_c = Arc::clone(&loader);
        let done_c = Arc::clone(&done);
        spawners.push(thread::spawn(move || {
            for i in 0..40usize {
                let url = if i % 10 == 0 {
                    format!("https://img.example/corrupt-{}.png", i % 5)
                } else {
                    format!("https://img.example/poster-{}.png", (t * 7 + i) % 12)
                };
                let done_cb = Arc::clone(&done_c);
                loader_c.request(
                    &url,
                    ArtworkHeaders::default(),
                    Box::new(move |res: ArtworkResult| {
                        let mut g = done_cb.0.lock().expect("done lock");
                        g.0 += 1;
                        if res.is_ok() {
                            g.1 += 1;
                        } else {
                            g.2 += 1;
                        }
                        if g.0 >= total_requests {
                            done_cb.1.notify_all();
                        }
                    }),
                );
            }
        }));
    }

    for h in spawners {
        h.join().expect("spawner finished");
    }

    let (lock, cvar) = &*done;
    let mut guard = lock.lock().expect("wait lock");
    while guard.0 < total_requests {
        let (next, timeout) = cvar
            .wait_timeout(guard, Duration::from_secs(5))
            .expect("condvar wait");
        guard = next;
        assert!(
            !timeout.timed_out(),
            "timed out waiting for artwork callbacks: got {}/{}",
            guard.0,
            total_requests
        );
    }
    assert_eq!(guard.0, total_requests);
    assert!(guard.1 > 0, "expected valid PNG callbacks");
    assert!(guard.2 > 0, "expected corrupt PNG error callbacks");
    drop(guard);
    drop(loader);
    let _ = std::fs::remove_dir_all(dir);
}
