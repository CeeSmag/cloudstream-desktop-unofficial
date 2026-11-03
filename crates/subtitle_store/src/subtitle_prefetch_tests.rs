//! Race, failure and ordering tests for the subtitle downloads: many files, many workers, files
//! queued while others download, a host that hangs, and the viewer leaving in the middle.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use crate::subtitle_cache::SubtitleCache;
use crate::subtitle_fetcher::{FetchError, SubtitleFetcher};
use crate::subtitle_loader::LoadOptions;

use super::*;

const LIMIT: Duration = Duration::from_secs(20);
static FOLDERS: AtomicU32 = AtomicU32::new(0);

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "cs_subtitle_prefetch_{}_{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::SeqCst)
        )))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A fetcher whose behavior is set per test: how long a download takes, which addresses fail.
struct Behavior {
    delay: Box<dyn Fn(&str) -> Duration + Send + Sync>,
    failing: Vec<&'static str>,
    started: Mutex<Vec<String>>,
    calls: AtomicUsize,
}

impl Behavior {
    fn instant() -> Self {
        Self::with_delay(|_| Duration::ZERO)
    }

    fn with_delay(delay: impl Fn(&str) -> Duration + Send + Sync + 'static) -> Self {
        Self {
            delay: Box::new(delay),
            failing: Vec::new(),
            started: Mutex::default(),
            calls: AtomicUsize::new(0),
        }
    }
}

impl SubtitleFetcher for Behavior {
    fn fetch(&self, url: &str, _: &[(String, String)]) -> Result<Vec<u8>, FetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.lock().expect("lock").push(url.to_owned());
        std::thread::sleep((self.delay)(url));
        if self.failing.iter().any(|failing| url.contains(failing)) {
            return Err(FetchError::Status { status: 404 });
        }
        Ok(format!("WEBVTT\n\n00:01.000 --> 00:02.000\n{url}\n").into_bytes())
    }
}

fn track(language: &str, url: &str) -> SubtitleTrack {
    SubtitleTrack {
        lang: language.to_owned(),
        url: url.to_owned(),
        headers: None,
    }
}

struct Run {
    prefetch: SubtitlePrefetch,
    handles: Vec<JoinHandle<()>>,
    ready: Arc<Mutex<Vec<ReadySubtitle>>>,
    wanted: Arc<AtomicBool>,
    _scratch: Scratch,
}

fn rank(language: &str) -> usize {
    match language {
        "Arabic" => 0,
        "English" => 1,
        _ => 9,
    }
}

fn start(fetcher: Arc<Behavior>, workers: usize) -> Run {
    let scratch = Scratch::new();
    let cache = SubtitleCache::new(scratch.0.clone(), Duration::from_secs(3600));
    let options = LoadOptions {
        attempts: 2,
        backoff: Duration::from_millis(1),
    };
    let loader = Arc::new(SubtitleLoader::new(cache, fetcher, options));
    let ready = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&ready);
    let wanted = Arc::new(AtomicBool::new(true));
    let asked = Arc::clone(&wanted);
    let config = PrefetchConfig {
        workers,
        rank_of: Arc::new(rank),
    };
    let (prefetch, handles) = SubtitlePrefetch::start(
        loader,
        &config,
        Arc::new(move |subtitle| sink.lock().expect("lock").push(subtitle)),
        Arc::new(move || asked.load(Ordering::SeqCst)),
    );
    Run {
        prefetch,
        handles,
        ready,
        wanted,
        _scratch: scratch,
    }
}

fn join(handles: Vec<JoinHandle<()>>) {
    let deadline = std::time::Instant::now() + LIMIT;
    for handle in handles {
        while !handle.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "a download thread did not end: deadlock"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        handle.join().expect("a download thread panicked");
    }
}

fn languages(run: &Run) -> Vec<String> {
    run.ready
        .lock()
        .expect("lock")
        .iter()
        .map(|s| s.language.clone())
        .collect()
}

#[test]
fn every_file_is_reported_ready_once_and_an_address_queued_twice_is_downloaded_once() {
    let fetcher = Arc::new(Behavior::instant());
    let mut run = start(Arc::clone(&fetcher), 4);
    run.prefetch.submit(vec![
        track("English", "https://s.test/en"),
        track("Arabic", "https://s.test/ar"),
    ]);
    run.prefetch.submit(vec![
        track("Arabic", "https://s.test/ar"),
        track("Spanish", "https://s.test/es"),
    ]);
    run.prefetch.finish();
    join(std::mem::take(&mut run.handles));
    assert_eq!(run.ready.lock().expect("lock").len(), 3);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 3);
    let summary = run.prefetch.summary();
    assert_eq!(
        (summary.submitted, summary.ready, summary.failed),
        (3, 3, 0)
    );
}

#[test]
fn the_preferred_languages_are_downloaded_first() {
    let mut run = start(Arc::new(Behavior::instant()), 1);
    run.prefetch.submit(vec![
        track("Spanish", "https://s.test/es"),
        track("English", "https://s.test/en"),
        track("German", "https://s.test/de"),
        track("Arabic", "https://s.test/ar"),
    ]);
    run.prefetch.finish();
    join(std::mem::take(&mut run.handles));
    assert_eq!(languages(&run), ["Arabic", "English", "Spanish", "German"]);
}

#[test]
fn files_that_fail_are_counted_by_reason_and_do_not_stop_the_others() {
    let mut behavior = Behavior::instant();
    behavior.failing = vec!["dead"];
    let mut run = start(Arc::new(behavior), 3);
    run.prefetch.submit(vec![
        track("Arabic", "https://s.test/dead1"),
        track("Arabic", "https://s.test/ok1"),
        track("English", "https://s.test/dead2"),
        track("English", "https://s.test/ok2"),
    ]);
    run.prefetch.finish();
    join(std::mem::take(&mut run.handles));
    let summary = run.prefetch.summary();
    assert_eq!((summary.ready, summary.failed), (2, 2));
    assert_eq!(summary.reasons.get("download failed (HTTP 404)"), Some(&2));
}

#[test]
fn one_file_that_hangs_does_not_hold_up_the_others() {
    let hung = Arc::new(AtomicBool::new(false));
    let marker = Arc::clone(&hung);
    let fetcher = Arc::new(Behavior::with_delay(move |url| {
        if url.ends_with("hangs") {
            marker.store(true, Ordering::SeqCst);
            Duration::from_millis(1_500)
        } else {
            Duration::from_millis(5)
        }
    }));
    let mut run = start(fetcher, 4);
    let mut tracks = vec![track("Arabic", "https://s.test/hangs")];
    tracks.extend((0..12).map(|n| track("English", &format!("https://s.test/en{n}"))));
    run.prefetch.submit(tracks);
    run.prefetch.finish();

    let deadline = std::time::Instant::now() + Duration::from_millis(1_000);
    while run.ready.lock().expect("lock").len() < 12 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        hung.load(Ordering::SeqCst),
        "the slow file was never started"
    );
    assert_eq!(
        run.ready.lock().expect("lock").len(),
        12,
        "the other files waited for the hung one"
    );
    join(std::mem::take(&mut run.handles));
}

#[test]
fn leaving_stops_the_downloads_and_ends_every_thread() {
    let fetcher = Arc::new(Behavior::with_delay(|_| Duration::from_millis(40)));
    let mut run = start(Arc::clone(&fetcher), 4);
    run.prefetch.submit(
        (0..200)
            .map(|n| track("English", &format!("https://s.test/{n}")))
            .collect(),
    );
    std::thread::sleep(Duration::from_millis(60));

    run.wanted.store(false, Ordering::SeqCst);
    run.prefetch.stop();

    join(std::mem::take(&mut run.handles));
    let started = fetcher.calls.load(Ordering::SeqCst);
    assert!(
        started < 40,
        "{started} downloads were started although nobody was waiting"
    );
}

#[path = "subtitle_prefetch_race_tests.rs"]
mod race;
