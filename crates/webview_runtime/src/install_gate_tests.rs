//! T1 and race tests for [`InstallGate`] and [`CancelFlag`].

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use super::*;

#[test]
fn a_second_press_is_refused_while_the_first_runs() {
    let gate = InstallGate::new();
    let run = gate.try_enter();
    assert!(run.is_some() && gate.is_busy());
    assert!(gate.clone().try_enter().is_none());
    drop(run);
    assert!(!gate.is_busy());
    assert!(gate.try_enter().is_some());
}

#[test]
fn a_flag_is_raised_for_every_clone() {
    let flag = CancelFlag::new();
    let seen_by_worker = flag.clone();
    assert!(!seen_by_worker.is_cancelled());
    flag.cancel();
    assert!(seen_by_worker.is_cancelled() && flag.is_cancelled());
}

/// 32 threads press Install 500 times each, all released together. Whoever gets in records that
/// it is inside; nobody may ever find someone else inside, and some presses must get in.
#[test]
fn race_presses_from_many_threads_never_run_two_installs() {
    const THREADS: usize = 32;
    const ROUNDS: usize = 500;
    let gate = InstallGate::new();
    let inside = Arc::new(AtomicUsize::new(0));
    let overlaps = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(AtomicUsize::new(0));
    let start = Arc::new(Barrier::new(THREADS));
    let workers: Vec<_> = (0..THREADS)
        .map(|_| {
            let (gate, inside, overlaps, entered, start) = (
                gate.clone(),
                Arc::clone(&inside),
                Arc::clone(&overlaps),
                Arc::clone(&entered),
                Arc::clone(&start),
            );
            thread::spawn(move || {
                start.wait();
                for _ in 0..ROUNDS {
                    let Some(run) = gate.try_enter() else {
                        continue;
                    };
                    if inside.fetch_add(1, Ordering::SeqCst) != 0 {
                        overlaps.fetch_add(1, Ordering::SeqCst);
                    }
                    entered.fetch_add(1, Ordering::SeqCst);
                    thread::yield_now();
                    inside.fetch_sub(1, Ordering::SeqCst);
                    drop(run);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(overlaps.load(Ordering::SeqCst), 0);
    assert!(entered.load(Ordering::SeqCst) > 0);
    assert!(!gate.is_busy());
}

/// The run ends on another thread than it started on (the worker drops it): the gate must open.
#[test]
fn race_a_run_dropped_by_the_worker_thread_opens_the_gate() {
    for _ in 0..200 {
        let gate = InstallGate::new();
        let run = gate.try_enter().unwrap();
        thread::spawn(move || drop(run)).join().unwrap();
        assert!(gate.try_enter().is_some());
    }
}
