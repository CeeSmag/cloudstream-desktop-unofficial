//! Race and behaviour tests of the poster loader with a scripted fetcher (no network).

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::{ArtworkError, ArtworkFetcher, ArtworkLoader, ArtworkLoaderConfig, ArtworkResult};

/// Counts downloads per address; can hold every download until released.
#[derive(Default)]
struct ScriptedFetcher {
    calls: Mutex<HashMap<String, usize>>,
    order: Mutex<Vec<String>>,
    held: Mutex<bool>,
    released: Condvar,
    failing: Mutex<Vec<String>>,
}

impl ScriptedFetcher {
    fn holding() -> Self {
        let fetcher = Self::default();
        *fetcher.held.lock().expect("held") = true;
        fetcher
    }

    fn release(&self) {
        *self.held.lock().expect("held") = false;
        self.released.notify_all();
    }

    fn calls_for(&self, url: &str) -> usize {
        self.calls
            .lock()
            .expect("calls")
            .get(url)
            .copied()
            .unwrap_or(0)
    }
}

fn tiny_png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(2, 3, image::Rgba([1, 2, 3, 255]));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("encode");
    bytes
}

impl ArtworkFetcher for ScriptedFetcher {
    fn fetch(&self, url: &str, _headers: &[(String, String)]) -> Result<Vec<u8>, ArtworkError> {
        self.order.lock().expect("order").push(url.to_owned());
        *self
            .calls
            .lock()
            .expect("calls")
            .entry(url.to_owned())
            .or_default() += 1;
        let mut held = self.held.lock().expect("held");
        while *held {
            held = self.released.wait(held).expect("wait");
        }
        drop(held);
        if self
            .failing
            .lock()
            .expect("failing")
            .iter()
            .any(|bad| bad == url)
        {
            return Err(ArtworkError::Status(404));
        }
        Ok(tiny_png())
    }
}

fn loader(fetcher: &Arc<ScriptedFetcher>, workers: usize) -> ArtworkLoader {
    let config = ArtworkLoaderConfig {
        workers,
        ..ArtworkLoaderConfig::default()
    };
    ArtworkLoader::new(Arc::clone(fetcher) as Arc<dyn ArtworkFetcher>, config)
}

/// A callback that reports into `answers`.
fn reporter(answers: &mpsc::Sender<ArtworkResult>) -> crate::ArtworkCallback {
    let answers = answers.clone();
    Box::new(move |result| answers.send(result).expect("send answer"))
}

fn wait_until(limit: Duration, condition: impl Fn() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    condition()
}

#[test]
fn request_many_threads_overlapping_urls_each_request_answered_once_each_url_fetched_once() {
    let fetcher = Arc::new(ScriptedFetcher::default());
    let loader = Arc::new(loader(&fetcher, 4));
    let answered = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..8)
        .map(|thread| {
            let (loader, answered) = (Arc::clone(&loader), Arc::clone(&answered));
            std::thread::spawn(move || {
                for round in 0..250 {
                    let url = format!("https://img.test/{}.png", (thread + round) % 20);
                    let answered = Arc::clone(&answered);
                    loader.request(
                        &url,
                        Vec::new(),
                        Box::new(move |result| {
                            assert!(result.is_ok(), "{result:?}");
                            answered.fetch_add(1, Ordering::SeqCst);
                        }),
                    );
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("requester");
    }
    assert!(wait_until(Duration::from_secs(5), || answered
        .load(Ordering::SeqCst)
        == 2000));
    for index in 0..20 {
        assert_eq!(
            fetcher.calls_for(&format!("https://img.test/{index}.png")),
            1
        );
    }
}

#[test]
fn request_same_url_while_loading_joins_the_running_download() {
    let fetcher = Arc::new(ScriptedFetcher::holding());
    let loader = loader(&fetcher, 2);
    let (answers, received) = mpsc::channel();
    loader.request("https://img.test/a.png", Vec::new(), reporter(&answers));
    assert!(wait_until(Duration::from_secs(2), || fetcher
        .calls_for("https://img.test/a.png")
        == 1));
    loader.request(
        "https://img.test/a.png",
        vec![("Referer".to_owned(), "https://site.test/".to_owned())],
        reporter(&answers),
    );
    fetcher.release();
    for _ in 0..2 {
        let result = received
            .recv_timeout(Duration::from_secs(2))
            .expect("answer");
        assert_eq!(
            result.map(|artwork| (artwork.width, artwork.height)),
            Ok((2, 3))
        );
    }
    assert_eq!(fetcher.calls_for("https://img.test/a.png"), 1);
    assert!(loader.cached("https://img.test/a.png").is_some());
}

#[test]
fn request_newest_is_served_first() {
    let fetcher = Arc::new(ScriptedFetcher::holding());
    let loader = loader(&fetcher, 1);
    let (answers, received) = mpsc::channel();
    loader.request("https://img.test/busy.png", Vec::new(), reporter(&answers));
    assert!(wait_until(Duration::from_secs(2), || fetcher
        .calls_for("https://img.test/busy.png")
        == 1));
    for name in ["old", "middle", "new"] {
        loader.request(
            &format!("https://img.test/{name}.png"),
            Vec::new(),
            reporter(&answers),
        );
    }
    fetcher.release();
    for _ in 0..4 {
        received
            .recv_timeout(Duration::from_secs(2))
            .expect("answer")
            .expect("ok");
    }
    let order = fetcher.order.lock().expect("order").clone();
    assert_eq!(
        order,
        ["busy", "new", "middle", "old"].map(|name| format!("https://img.test/{name}.png"))
    );
}

#[test]
fn request_failed_download_answers_every_waiter_and_is_not_remembered() {
    let fetcher = Arc::new(ScriptedFetcher::holding());
    fetcher
        .failing
        .lock()
        .expect("failing")
        .push("https://img.test/gone.png".into());
    let loader = loader(&fetcher, 1);
    let (answers, received) = mpsc::channel();
    loader.request("https://img.test/gone.png", Vec::new(), reporter(&answers));
    loader.request("https://img.test/gone.png", Vec::new(), reporter(&answers));
    fetcher.release();
    for _ in 0..2 {
        let result = received
            .recv_timeout(Duration::from_secs(2))
            .expect("answer");
        assert_eq!(result.map(|_| ()), Err(ArtworkError::Status(404)));
    }
    assert!(loader.cached("https://img.test/gone.png").is_none());
    loader.request("https://img.test/gone.png", Vec::new(), reporter(&answers));
    received
        .recv_timeout(Duration::from_secs(2))
        .expect("answer")
        .expect_err("still gone");
    assert_eq!(
        fetcher.calls_for("https://img.test/gone.png"),
        2,
        "a failure is tried again"
    );
}

#[test]
fn shutdown_with_queued_and_loading_requests_answers_each_once_and_workers_end() {
    let fetcher = Arc::new(ScriptedFetcher::holding());
    let loader = loader(&fetcher, 2);
    let (answers, received) = mpsc::channel();
    for index in 0..10 {
        loader.request(
            &format!("https://img.test/{index}.png"),
            Vec::new(),
            reporter(&answers),
        );
    }
    assert!(wait_until(Duration::from_secs(2), || fetcher
        .order
        .lock()
        .expect("order")
        .len()
        == 2));
    loader.shutdown();
    fetcher.release();
    let mut count = 0;
    while let Ok(result) = received.recv_timeout(Duration::from_millis(300)) {
        assert_eq!(result.map(|_| ()), Err(ArtworkError::ShutDown));
        count += 1;
    }
    assert_eq!(count, 10, "each request answered exactly once");
    assert!(wait_until(Duration::from_secs(2), || loader.live_workers() == 0));
    loader.request("https://img.test/late.png", Vec::new(), reporter(&answers));
    let late = received
        .recv_timeout(Duration::from_secs(1))
        .expect("late answer");
    assert_eq!(late.map(|_| ()), Err(ArtworkError::ShutDown));
}

#[test]
fn request_unsupported_address_is_answered_at_once_without_download() {
    let fetcher = Arc::new(ScriptedFetcher::default());
    let loader = loader(&fetcher, 1);
    let (answers, received) = mpsc::channel();
    loader.request("data:image/png;base64,AA", Vec::new(), reporter(&answers));
    let result = received
        .recv_timeout(Duration::from_secs(1))
        .expect("answer");
    assert!(matches!(result, Err(ArtworkError::Unsupported(_))));
    assert!(fetcher.order.lock().expect("order").is_empty());
}

fn disk_folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cs_art_ephem_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn count_img_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "img"))
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn ephemeral_request_never_writes_disk_and_forget_memory_drops_decode() {
    let dir = disk_folder("skip");
    let fetcher = Arc::new(ScriptedFetcher::default());
    let config = ArtworkLoaderConfig {
        workers: 1,
        disk: Some(crate::DiskCache::new(&dir)),
        ..ArtworkLoaderConfig::default()
    };
    let loader = ArtworkLoader::new(Arc::clone(&fetcher) as Arc<dyn ArtworkFetcher>, config);
    let (answers, received) = mpsc::channel();
    let url = "https://img.test/details.png";
    loader.request_ephemeral(url, Vec::new(), reporter(&answers));
    received
        .recv_timeout(Duration::from_secs(2))
        .expect("answer")
        .expect("ok");
    assert_eq!(count_img_files(&dir), 0, "ephemeral must not save to disk");
    assert!(loader.cached(url).is_some());
    loader.forget_memory(url);
    assert!(loader.cached(url).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn persistent_request_writes_disk_while_ephemeral_skips_existing_file() {
    let dir = disk_folder("persist");
    let fetcher = Arc::new(ScriptedFetcher::default());
    let config = ArtworkLoaderConfig {
        workers: 1,
        disk: Some(crate::DiskCache::new(&dir)),
        ..ArtworkLoaderConfig::default()
    };
    let loader = ArtworkLoader::new(Arc::clone(&fetcher) as Arc<dyn ArtworkFetcher>, config);
    let (answers, received) = mpsc::channel();
    let url = "https://img.test/home.png";
    loader.request(url, Vec::new(), reporter(&answers));
    received
        .recv_timeout(Duration::from_secs(2))
        .expect("answer")
        .expect("ok");
    assert_eq!(count_img_files(&dir), 1);
    assert_eq!(fetcher.calls_for(url), 1);
    loader.forget_memory(url);
    // Ephemeral must not reuse the disk file; it downloads again and still leaves disk alone.
    loader.request_ephemeral(url, Vec::new(), reporter(&answers));
    received
        .recv_timeout(Duration::from_secs(2))
        .expect("answer")
        .expect("ok");
    assert_eq!(fetcher.calls_for(url), 2);
    assert_eq!(count_img_files(&dir), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
