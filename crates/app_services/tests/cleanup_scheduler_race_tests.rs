//! Scheduler lifecycle and mandatory concurrency race tests (RULES.md §9) for `CleanupScheduler`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use app_services::{CleanupMode, CleanupReport, CleanupSchedule, CleanupScheduler, CleanupTask};

struct RecordingTask {
    runs: Mutex<Vec<(CleanupMode, Instant)>>,
    busy: AtomicBool,
    in_flight: AtomicUsize,
    max_overlap: AtomicUsize,
    work_delay: Duration,
    notify: Mutex<Option<mpsc::Sender<CleanupMode>>>,
}

impl RecordingTask {
    fn new(work_delay: Duration) -> (Arc<Self>, mpsc::Receiver<CleanupMode>) {
        let (tx, rx) = mpsc::channel();
        let task = Arc::new(Self {
            runs: Mutex::new(Vec::new()),
            busy: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            max_overlap: AtomicUsize::new(0),
            work_delay,
            notify: Mutex::new(Some(tx)),
        });
        (task, rx)
    }

    fn count(&self, mode: CleanupMode) -> usize {
        self.runs
            .lock()
            .map(|runs| runs.iter().filter(|(kind, _)| *kind == mode).count())
            .unwrap_or(0)
    }
}

impl CleanupTask for RecordingTask {
    fn run(&self, mode: CleanupMode) -> CleanupReport {
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_overlap.fetch_max(current, Ordering::SeqCst);
        if !self.work_delay.is_zero() {
            thread::sleep(self.work_delay);
        }
        if let Ok(mut runs) = self.runs.lock() {
            runs.push((mode, Instant::now()));
        }
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        if let Ok(guard) = self.notify.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(mode);
            }
        }
        CleanupReport {
            files: 1,
            bytes: 1024,
        }
    }

    fn is_playback_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
}

struct GatedPeriodicTask {
    in_flight: AtomicUsize,
    max_overlap: AtomicUsize,
    periodic_entered: Mutex<Option<mpsc::Sender<()>>>,
    release_periodic: Mutex<Option<mpsc::Receiver<()>>>,
    events: Mutex<Vec<(CleanupMode, Instant)>>,
    notify: mpsc::Sender<CleanupMode>,
}

impl CleanupTask for GatedPeriodicTask {
    fn run(&self, mode: CleanupMode) -> CleanupReport {
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_overlap.fetch_max(current, Ordering::SeqCst);
        if mode == CleanupMode::Periodic {
            if let Ok(mut entered) = self.periodic_entered.lock() {
                if let Some(tx) = entered.take() {
                    let _ = tx.send(());
                }
            }
            if let Ok(mut release) = self.release_periodic.lock() {
                if let Some(rx) = release.take() {
                    let _ = rx.recv_timeout(Duration::from_secs(2));
                }
            }
        }
        if mode == CleanupMode::Manual {
            // Take noticeable time so resetting the hourly clock after manual differs cleanly
            // from keeping the earlier periodic completion deadline.
            thread::sleep(Duration::from_millis(120));
        }
        if let Ok(mut events) = self.events.lock() {
            events.push((mode, Instant::now()));
        }
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let _ = self.notify.send(mode);
        CleanupReport {
            files: 2,
            bytes: 2048,
        }
    }

    fn is_playback_busy(&self) -> bool {
        false
    }
}

#[test]
fn nothing_runs_before_home_ready_then_runs_after_delay_and_repeats_hourly() {
    let (task, rx) = RecordingTask::new(Duration::ZERO);
    let schedule = CleanupSchedule {
        post_home_delay: Duration::from_millis(35),
        hourly_interval: Duration::from_millis(70),
        busy_retry_delay: Duration::from_millis(30),
    };
    let scheduler = CleanupScheduler::start(Arc::clone(&task) as Arc<dyn CleanupTask>, schedule);
    let handle = scheduler.handle();

    // Nothing runs until Home is ready.
    assert!(rx.recv_timeout(Duration::from_millis(80)).is_err());
    assert_eq!(task.count(CleanupMode::Periodic), 0);

    // Notify Home ready (calling twice must be idempotent).
    let ready_at = Instant::now();
    handle.notify_home_ready();
    handle.notify_home_ready();

    let first = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first periodic");
    assert_eq!(first, CleanupMode::Periodic);
    assert!(
        ready_at.elapsed() >= Duration::from_millis(25),
        "first cleanup should wait for post_home_delay"
    );

    let second = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("second periodic");
    assert_eq!(second, CleanupMode::Periodic);
    drop(scheduler);
}

#[test]
fn playback_busy_defers_automatic_pass_and_retries_later() {
    let (task, rx) = RecordingTask::new(Duration::ZERO);
    task.busy.store(true, Ordering::SeqCst);
    let schedule = CleanupSchedule {
        post_home_delay: Duration::from_millis(20),
        hourly_interval: Duration::from_millis(500),
        busy_retry_delay: Duration::from_millis(30),
    };
    let scheduler = CleanupScheduler::start(Arc::clone(&task) as Arc<dyn CleanupTask>, schedule);
    scheduler.handle().notify_home_ready();

    // Deferred while playback is active.
    assert!(rx.recv_timeout(Duration::from_millis(80)).is_err());
    assert_eq!(task.count(CleanupMode::Periodic), 0);

    // Once playback stops, the retry triggers cleanly.
    task.busy.store(false, Ordering::SeqCst);
    let mode = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("retried periodic");
    assert_eq!(mode, CleanupMode::Periodic);
    drop(scheduler);
}

#[test]
fn manual_cleanup_resets_hourly_timer_and_cancels_pending_post_home_run() {
    let (task, rx) = RecordingTask::new(Duration::ZERO);
    let schedule = CleanupSchedule {
        post_home_delay: Duration::from_millis(60),
        hourly_interval: Duration::from_millis(160),
        busy_retry_delay: Duration::from_millis(40),
    };
    let scheduler = CleanupScheduler::start(Arc::clone(&task) as Arc<dyn CleanupTask>, schedule);
    let handle = scheduler.handle();

    handle.notify_home_ready();
    thread::sleep(Duration::from_millis(15));
    let manual_done_at = Instant::now();
    let report = handle.run_manual_now();
    assert_eq!(
        report,
        CleanupReport {
            files: 1,
            bytes: 1024
        }
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1)).expect("manual"),
        CleanupMode::Manual
    );

    // The 60 ms post-home timer must NOT fire now; the hourly timer was reset to 160 ms from manual.
    assert!(
        rx.recv_timeout(Duration::from_millis(90)).is_err(),
        "post-home timer was not reset by manual cleanup"
    );

    let next = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("next hourly after reset");
    assert_eq!(next, CleanupMode::Periodic);
    assert!(
        manual_done_at.elapsed() >= Duration::from_millis(140),
        "next periodic fired too early after manual reset: {:?}",
        manual_done_at.elapsed()
    );
    drop(scheduler);
}

#[test]
fn enforce_cap_now_runs_without_resetting_hourly_timer() {
    let (task, rx) = RecordingTask::new(Duration::ZERO);
    let schedule = CleanupSchedule {
        post_home_delay: Duration::from_millis(80),
        hourly_interval: Duration::from_millis(500),
        busy_retry_delay: Duration::from_millis(50),
    };
    let scheduler = CleanupScheduler::start(Arc::clone(&task) as Arc<dyn CleanupTask>, schedule);
    let handle = scheduler.handle();

    let started = Instant::now();
    handle.notify_home_ready();
    let cap_report = handle.enforce_cap_now();
    assert_eq!(
        cap_report,
        CleanupReport {
            files: 1,
            bytes: 1024
        }
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1)).expect("cap"),
        CleanupMode::CapOnly
    );

    // Original 80 ms post-home periodic pass should still fire on schedule (not pushed to 500 ms).
    let periodic = rx
        .recv_timeout(Duration::from_millis(250))
        .expect("periodic should not be pushed to hourly_interval");
    assert_eq!(periodic, CleanupMode::Periodic);
    assert!(started.elapsed() < Duration::from_millis(350));
    drop(scheduler);
}

struct GatedFixture {
    task: Arc<GatedPeriodicTask>,
    entered_rx: mpsc::Receiver<()>,
    release_tx: mpsc::Sender<()>,
    event_rx: mpsc::Receiver<CleanupMode>,
}

fn build_gated_task() -> GatedFixture {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let task = Arc::new(GatedPeriodicTask {
        in_flight: AtomicUsize::new(0),
        max_overlap: AtomicUsize::new(0),
        periodic_entered: Mutex::new(Some(entered_tx)),
        release_periodic: Mutex::new(Some(release_rx)),
        events: Mutex::new(Vec::new()),
        notify: event_tx,
    });
    GatedFixture {
        task,
        entered_rx,
        release_tx,
        event_rx,
    }
}

impl GatedFixture {
    fn next_event(&self, timeout: Duration) -> Option<CleanupMode> {
        self.event_rx.recv_timeout(timeout).ok()
    }
}

/// Mandatory deterministic race test (RULES.md §9):
/// A manual cleanup request arriving while a periodic pass is in-flight must wait without overlap,
/// run right after the periodic pass finishes, and reset the next hourly tick from manual completion.
#[test]
fn deterministic_interleaving_manual_mid_periodic_serializes_and_resets_hourly_clock() {
    let fixture = build_gated_task();
    let schedule = CleanupSchedule {
        post_home_delay: Duration::from_millis(10),
        hourly_interval: Duration::from_millis(180),
        busy_retry_delay: Duration::from_millis(50),
    };
    let scheduler =
        CleanupScheduler::start(Arc::clone(&fixture.task) as Arc<dyn CleanupTask>, schedule);
    let handle = scheduler.handle();

    handle.notify_home_ready();
    fixture
        .entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("periodic entered");

    let manual_handle = handle.clone();
    let manual_thread = thread::spawn(move || manual_handle.run_manual_now());
    thread::sleep(Duration::from_millis(30));
    assert_eq!(fixture.task.max_overlap.load(Ordering::SeqCst), 1);

    fixture.release_tx.send(()).expect("release periodic");
    let report = manual_thread.join().expect("manual joined");
    let manual_finished = Instant::now();
    assert_eq!(report.files, 2);
    assert_eq!(
        fixture.next_event(Duration::from_secs(1)),
        Some(CleanupMode::Periodic)
    );
    assert_eq!(
        fixture.next_event(Duration::from_secs(1)),
        Some(CleanupMode::Manual)
    );
    assert_eq!(fixture.next_event(Duration::from_millis(100)), None);
    assert_eq!(
        fixture.next_event(Duration::from_secs(1)),
        Some(CleanupMode::Periodic)
    );
    assert!(manual_finished.elapsed() >= Duration::from_millis(150));
    assert_eq!(fixture.task.max_overlap.load(Ordering::SeqCst), 1);
    drop(scheduler);
}

/// Mandatory multi-threaded stress race test (RULES.md §9):
/// Many threads concurrently call `notify_home_ready`, `run_manual_now`, and `enforce_cap_now`
/// while periodic ticks fire and then the scheduler shuts down mid-storm.
#[test]
fn stress_concurrent_calls_and_shutdown_never_overlap_or_deadlock() {
    let (task, _rx) = RecordingTask::new(Duration::from_millis(2));
    let schedule = CleanupSchedule {
        post_home_delay: Duration::from_millis(5),
        hourly_interval: Duration::from_millis(10),
        busy_retry_delay: Duration::from_millis(5),
    };
    let scheduler = CleanupScheduler::start(Arc::clone(&task) as Arc<dyn CleanupTask>, schedule);
    let handle = scheduler.handle();

    let mut threads = Vec::new();
    for worker_id in 0..10 {
        let h = handle.clone();
        threads.push(thread::spawn(move || {
            for round in 0..8 {
                match (worker_id + round) % 3 {
                    0 => h.notify_home_ready(),
                    1 => {
                        let _ = h.run_manual_now();
                    }
                    _ => {
                        let _ = h.enforce_cap_now();
                    }
                }
            }
        }));
    }

    // Let contention run briefly, then drop scheduler mid-flight to verify clean shutdown.
    thread::sleep(Duration::from_millis(35));
    drop(scheduler);

    for t in threads {
        t.join().expect("stress worker must not panic or deadlock");
    }
    assert!(
        task.max_overlap.load(Ordering::SeqCst) <= 1,
        "cleanup tasks must never overlap"
    );
    assert_eq!(task.in_flight.load(Ordering::SeqCst), 0);
}
