//! Race tests for the subtitle downloads: files queued from several threads at once and the
//! viewer leaving at any moment.

use super::*;

/// Files are queued from several threads while the workers download: every address is downloaded
/// exactly once and reported exactly once, whichever thread queued it.
#[test]
fn under_concurrent_queueing_every_file_is_downloaded_and_reported_exactly_once() {
    const QUEUERS: usize = 4;
    const PER_QUEUER: usize = 150;
    let fetcher = Arc::new(Behavior::instant());
    let mut run = start(Arc::clone(&fetcher), 8);
    let queuers: Vec<_> = (0..QUEUERS)
        .map(|queuer| {
            let prefetch = run.prefetch.clone();
            std::thread::spawn(move || {
                for n in 0..PER_QUEUER {
                    // Every second address is also queued by the next thread: overlap on purpose.
                    let id = queuer * PER_QUEUER / 2 + n;
                    prefetch.submit(vec![track("English", &format!("https://s.test/{id}"))]);
                }
            })
        })
        .collect();
    join(queuers);
    run.prefetch.finish();
    join(std::mem::take(&mut run.handles));

    let ready = run.ready.lock().expect("lock");
    let mut urls: Vec<&str> = ready.iter().map(|s| s.url.as_str()).collect();
    urls.sort_unstable();
    let total = urls.len();
    urls.dedup();
    assert_eq!(urls.len(), total, "a file was reported ready twice");
    let expected = (QUEUERS - 1) * PER_QUEUER / 2 + PER_QUEUER;
    assert_eq!(total, expected, "a file was lost");
    assert_eq!(
        fetcher.calls.load(Ordering::SeqCst),
        expected,
        "a file was downloaded twice"
    );
    let by_url: HashMap<_, _> = ready
        .iter()
        .map(|s| (s.url.clone(), s.path.clone()))
        .collect();
    assert!(
        by_url.values().all(|path| path.exists()),
        "a reported file is not on disk"
    );
}

/// The viewer leaves at every point of the downloads: every thread ends and nothing is reported
/// after the stop.
#[test]
fn stopping_at_any_moment_ends_every_thread() {
    for delay_micros in [0u64, 50, 300, 1_000, 4_000, 15_000] {
        let fetcher = Arc::new(Behavior::with_delay(|_| Duration::from_millis(2)));
        let mut run = start(fetcher, 6);
        run.prefetch.submit(
            (0..60)
                .map(|n| track("Arabic", &format!("https://s.test/{n}")))
                .collect(),
        );
        std::thread::sleep(Duration::from_micros(delay_micros));
        run.wanted.store(false, Ordering::SeqCst);
        run.prefetch.stop();
        join(std::mem::take(&mut run.handles));
        let before = run.ready.lock().expect("lock").len();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            run.ready.lock().expect("lock").len(),
            before,
            "a file was reported after the stop"
        );
        let summary = run.prefetch.summary();
        assert_eq!(
            (summary.queued, summary.in_flight),
            (0, 0),
            "nothing remains queued or in flight after threads join"
        );
    }
}

fn spawn_pause_toggler(prefetch: &SubtitlePrefetch) -> JoinHandle<()> {
    let prefetch = prefetch.clone();
    std::thread::spawn(move || {
        for i in 0..80 {
            prefetch.set_paused(i % 2 == 0);
            std::thread::yield_now();
        }
        prefetch.set_paused(false);
    })
}

/// Concurrent submissions, pause/resume toggles, and failing/healthy hosts report every submitted
/// track via either `on_ready` or `on_failed` exactly once with zero deadlocks.
#[test]
fn concurrent_pause_resume_and_host_failures_report_every_track_once() {
    const TOTAL_PER_HOST: usize = 40;
    let scratch = Scratch::new();
    let mut behavior = Behavior::instant();
    behavior.failing = vec!["bad.test"];
    let loader = loader_over(&scratch, Arc::new(behavior));
    let ready = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let (ready_cb, failed_cb) = (Arc::clone(&ready), Arc::clone(&failed));
    let config = PrefetchConfig {
        workers: 6,
        rank_of: Arc::new(rank),
    };
    let callbacks = PrefetchCallbacks {
        on_ready: Arc::new(move |_| {
            ready_cb.fetch_add(1, Ordering::SeqCst);
        }),
        on_failed: Arc::new(move |_, _| {
            failed_cb.fetch_add(1, Ordering::SeqCst);
        }),
        keep_going: Arc::new(|| true),
    };
    let (prefetch, handles) = SubtitlePrefetch::start_with_callbacks(loader, &config, callbacks);
    let toggler = spawn_pause_toggler(&prefetch);
    for n in 0..TOTAL_PER_HOST {
        prefetch.submit(vec![
            track("English", &format!("https://bad.test/{n}.vtt")),
            track("English", &format!("https://good.test/{n}.vtt")),
        ]);
    }
    toggler.join().expect("toggler");
    prefetch.finish();
    join(handles);
    assert_eq!(ready.load(Ordering::SeqCst), TOTAL_PER_HOST);
    assert_eq!(failed.load(Ordering::SeqCst), TOTAL_PER_HOST);
}
