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
    }
}
