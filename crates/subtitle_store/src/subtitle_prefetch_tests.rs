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
    start_ranked(fetcher, workers, Arc::new(rank))
}

fn start_ranked(fetcher: Arc<Behavior>, workers: usize, rank_of: LanguageRank) -> Run {
    let scratch = Scratch::new();
    let cache = Arc::new(SubtitleCache::new(
        scratch.0.clone(),
        Duration::from_secs(3600),
    ));
    let options = LoadOptions {
        attempts: 2,
        backoff: Duration::from_millis(1),
    };
    let as_fetcher: Arc<dyn SubtitleFetcher> = fetcher;
    let loader = Arc::new(SubtitleLoader::new(cache, as_fetcher, options));
    let ready = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&ready);
    let wanted = Arc::new(AtomicBool::new(true));
    let asked = Arc::clone(&wanted);
    let config = PrefetchConfig { workers, rank_of };
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
fn languages_ranked_skip_download_are_not_queued() {
    let behavior = Arc::new(Behavior::instant());
    let mut run = start_ranked(
        Arc::clone(&behavior),
        2,
        Arc::new(|label| {
            if label == "English" {
                0
            } else {
                SKIP_DOWNLOAD_RANK
            }
        }),
    );
    run.prefetch.submit(vec![
        track("Spanish", "https://s.test/es"),
        track("English", "https://s.test/en"),
        track("German", "https://s.test/de"),
    ]);
    run.prefetch.finish();
    join(std::mem::take(&mut run.handles));
    assert_eq!(languages(&run), ["English"]);
    assert_eq!(behavior.calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.prefetch.summary().submitted, 1);
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

/// A fetcher that holds every download until the test opens it, and says when one is held.
#[derive(Default)]
struct Gate {
    state: Mutex<(bool, usize)>,
    changed: std::sync::Condvar,
}

impl Gate {
    fn open(&self) {
        self.state.lock().expect("lock").0 = true;
        self.changed.notify_all();
    }

    /// Waits until `count` downloads are being held.
    fn wait_held(&self, count: usize) {
        let guard = self.state.lock().expect("lock");
        let (_guard, wait) = self
            .changed
            .wait_timeout_while(guard, LIMIT, |(_, held)| *held < count)
            .expect("lock");
        assert!(!wait.timed_out(), "no download started");
    }
}

impl SubtitleFetcher for Gate {
    fn fetch(&self, url: &str, _: &[(String, String)]) -> Result<Vec<u8>, FetchError> {
        let mut state = self.state.lock().expect("lock");
        state.1 += 1;
        self.changed.notify_all();
        let _state = self
            .changed
            .wait_while(state, |(open, _)| !*open)
            .expect("lock");
        Ok(format!("WEBVTT\n\n00:01.000 --> 00:02.000\n{url}\n").into_bytes())
    }
}

/// Starts downloads over `loader` with one worker and every file wanted.
fn start_over(loader: &Arc<SubtitleLoader>) -> (SubtitlePrefetch, Vec<JoinHandle<()>>) {
    let config = PrefetchConfig {
        workers: 1,
        rank_of: Arc::new(rank),
    };
    SubtitlePrefetch::start(
        Arc::clone(loader),
        &config,
        Arc::new(|_| {}),
        Arc::new(|| true),
    )
}

fn loader_over(scratch: &Scratch, fetcher: Arc<dyn SubtitleFetcher>) -> Arc<SubtitleLoader> {
    let cache = Arc::new(SubtitleCache::new(
        scratch.0.clone(),
        Duration::from_secs(3600),
    ));
    Arc::new(SubtitleLoader::new(cache, fetcher, LoadOptions::default()))
}

#[test]
fn the_summary_counts_a_download_in_progress_as_in_flight() {
    let scratch = Scratch::new();
    let gate = Arc::new(Gate::default());
    let as_fetcher: Arc<dyn SubtitleFetcher> = gate.clone();
    let (prefetch, handles) = start_over(&loader_over(&scratch, as_fetcher));
    prefetch.submit(vec![
        track("Arabic", "https://s.test/ar"),
        track("English", "https://s.test/en"),
    ]);
    gate.wait_held(1);

    let during = prefetch.summary();
    gate.open();
    prefetch.finish();
    join(handles);

    assert_eq!(
        (during.in_flight, during.queued),
        (1, 1),
        "one worker, one held"
    );
    let after = prefetch.summary();
    assert_eq!((after.in_flight, after.queued, after.ready), (0, 0, 2));
}

#[test]
fn files_already_on_disk_are_counted_as_from_the_cache() {
    let scratch = Scratch::new();
    let loader = loader_over(&scratch, Arc::new(Behavior::instant()));
    let tracks = vec![
        track("Arabic", "https://s.test/ar"),
        track("English", "https://s.test/en"),
    ];
    let (first, handles) = start_over(&loader);
    first.submit(tracks.clone());
    first.finish();
    join(handles);
    assert_eq!((first.summary().ready, first.summary().from_cache), (2, 0));

    // The same video opened again: both files are on disk now.
    let (second, handles) = start_over(&loader);
    second.submit(tracks);
    second.finish();
    join(handles);
    assert_eq!(
        (second.summary().ready, second.summary().from_cache),
        (2, 2)
    );
}

#[test]
fn host_circuit_skips_remaining_urls_on_failing_host_and_reports_failures() {
    let scratch = Scratch::new();
    let mut behavior = Behavior::instant();
    behavior.failing = vec!["bad.test"];
    let fetcher = Arc::new(behavior);
    let loader = loader_over(&scratch, Arc::clone(&fetcher) as Arc<dyn SubtitleFetcher>);
    let ready = Arc::new(Mutex::new(Vec::<String>::new()));
    let failed = Arc::new(Mutex::new(Vec::<String>::new()));
    let ready_cb = Arc::clone(&ready);
    let failed_cb = Arc::clone(&failed);
    let config = PrefetchConfig {
        workers: 1,
        rank_of: Arc::new(rank),
    };
    let (prefetch, handles) = SubtitlePrefetch::start_with_callbacks(
        loader,
        &config,
        PrefetchCallbacks {
            on_ready: Arc::new(move |sub| ready_cb.lock().expect("lock").push(sub.url)),
            on_failed: Arc::new(move |t, _| failed_cb.lock().expect("lock").push(t.url.clone())),
            keep_going: Arc::new(|| true),
        },
    );
    let mut batch: Vec<_> = (0..10)
        .map(|n| track("English", &format!("https://bad.test/{n}.vtt")))
        .collect();
    batch.push(track("English", "https://good.test/ok.vtt"));
    prefetch.submit(batch);
    prefetch.finish();
    join(handles);

    assert_eq!(ready.lock().expect("lock").len(), 1);
    assert_eq!(failed.lock().expect("lock").len(), 10);
    // 3 failing HTTP calls trip the host circuit + 1 healthy call = 4 total (not 11).
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 4);
    assert_eq!(
        prefetch.summary().reasons.get("host skipped").copied(),
        Some(7)
    );
}

#[test]
fn set_paused_holds_queued_downloads_until_resumed_or_stopped() {
    let scratch = Scratch::new();
    let fetcher = Arc::new(Behavior::instant());
    let loader = loader_over(&scratch, Arc::clone(&fetcher) as Arc<dyn SubtitleFetcher>);
    let (prefetch, handles) = start_over(&loader);

    prefetch.set_paused(true);
    prefetch.submit(vec![
        track("English", "https://good.test/1.vtt"),
        track("English", "https://good.test/2.vtt"),
    ]);
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);

    prefetch.set_paused(false);
    prefetch.finish();
    join(handles);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
    assert_eq!(prefetch.summary().ready, 2);

    // Stopping while paused exits every worker cleanly without deadlock.
    let (paused_stop, stop_handles) = start_over(&loader);
    paused_stop.set_paused(true);
    paused_stop.submit(vec![track("English", "https://good.test/3.vtt")]);
    paused_stop.stop();
    join(stop_handles);
}
