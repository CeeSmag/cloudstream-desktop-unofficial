//! Race and scheduling tests for `CleanupHandle`.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn run_now_wins_over_reset_when_both_arrive() {
    let handle = CleanupHandle::new();
    handle.mark_alive();
    handle.reset_schedule();
    handle.request_now();
    assert_eq!(handle.take_pending(), Some(Pending::RunNow));
    assert_eq!(handle.take_pending(), None);
}

#[test]
fn many_threads_signalling_leave_one_clear_pending() {
    let handle = CleanupHandle::new();
    handle.mark_alive();
    let runs = Arc::new(AtomicUsize::new(0));
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let handle = handle.clone();
            let runs = Arc::clone(&runs);
            thread::spawn(move || {
                for _ in 0..200 {
                    if i % 2 == 0 {
                        handle.request_now();
                    } else {
                        handle.reset_schedule();
                    }
                    runs.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("worker");
    }
    assert_eq!(runs.load(Ordering::SeqCst), 8 * 200);
    let pending = handle.take_pending();
    assert!(
        pending == Some(Pending::RunNow) || pending == Some(Pending::ResetSchedule),
        "unexpected {pending:?}"
    );
    assert_eq!(handle.take_pending(), None);
}

#[test]
fn wait_returns_early_when_signalled() {
    let handle = CleanupHandle::new();
    handle.mark_alive();
    let waiter = {
        let handle = handle.clone();
        thread::spawn(move || {
            let started = std::time::Instant::now();
            handle.wait(Duration::from_secs(5));
            started.elapsed()
        })
    };
    thread::sleep(Duration::from_millis(20));
    handle.request_now();
    let waited = waiter.join().expect("waiter");
    assert!(
        waited < Duration::from_secs(2),
        "wait did not wake early: {waited:?}"
    );
}
