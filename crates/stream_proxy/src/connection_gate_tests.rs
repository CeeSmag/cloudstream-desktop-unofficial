use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Barrier;

use super::*;

const LIMIT: usize = 4;
const THREADS: usize = 16;
const ROUNDS: usize = 2_000;

/// A deadlock must fail a test, not hang the run.
fn finishes_within_limit(body: impl FnOnce() + Send + 'static) {
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        body();
        let _ = done.send(());
    });
    assert!(
        finished.recv_timeout(Duration::from_secs(20)).is_ok(),
        "the test body did not finish: a deadlock, or a panic in a worker"
    );
}

#[test]
fn try_enter_admits_up_to_the_limit_and_a_dropped_permit_frees_a_place() {
    let gate = ConnectionGate::new(2);
    let first = gate.try_enter().expect("a place");
    let _second = gate.try_enter().expect("a place");
    assert!(gate.try_enter().is_none(), "the gate is full");
    assert_eq!(gate.open_count(), 2);
    drop(first);
    assert!(gate.try_enter().is_some(), "a place was given back");
}

#[test]
fn release_then_gives_the_place_back_before_it_runs_the_closing_step() {
    let gate = ConnectionGate::new(1);
    let permit = gate.try_enter().expect("a place");
    let mut open_when_closing = None;
    permit.release_then(|| open_when_closing = Some(gate.open_count()));
    assert_eq!(
        open_when_closing,
        Some(0),
        "the place was still taken while closing"
    );
}

#[test]
fn wait_idle_returns_at_once_when_nothing_is_open_and_gives_up_when_something_stays() {
    let gate = ConnectionGate::new(2);
    assert!(gate.wait_idle(Duration::ZERO));
    let permit = gate.try_enter().expect("a place");
    assert!(
        !gate.wait_idle(Duration::from_millis(20)),
        "a connection is still open"
    );
    drop(permit);
    assert!(gate.wait_idle(Duration::ZERO));
}

#[test]
fn wait_idle_wakes_when_the_last_permit_is_dropped_on_another_thread() {
    let gate = ConnectionGate::new(2);
    let permit = gate.try_enter().expect("a place");
    let ready = Arc::new(Barrier::new(2));
    let signal = Arc::clone(&ready);
    let dropper = std::thread::spawn(move || {
        signal.wait();
        drop(permit);
    });
    ready.wait();
    assert!(
        gate.wait_idle(Duration::from_secs(10)),
        "the drop must wake the waiter"
    );
    dropper.join().expect("dropper finished");
}

#[test]
fn many_threads_entering_and_leaving_never_exceed_the_limit_and_end_with_none_open() {
    finishes_within_limit(|| {
        let gate = ConnectionGate::new(LIMIT);
        let inside = Arc::new(AtomicUsize::new(0));
        let admitted = Arc::new(AtomicUsize::new(0));
        let workers: Vec<_> = (0..THREADS)
            .map(|_| {
                let (gate, inside, admitted) = (
                    Arc::clone(&gate),
                    Arc::clone(&inside),
                    Arc::clone(&admitted),
                );
                std::thread::spawn(move || {
                    for _ in 0..ROUNDS {
                        if let Some(permit) = gate.try_enter() {
                            let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                            assert!(now <= LIMIT, "{now} connections inside a gate of {LIMIT}");
                            admitted.fetch_add(1, Ordering::SeqCst);
                            // Stay inside long enough for the others to pile up at the gate.
                            std::thread::yield_now();
                            inside.fetch_sub(1, Ordering::SeqCst);
                            drop(permit);
                        }
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("worker finished");
        }
        assert!(admitted.load(Ordering::SeqCst) > 0, "somebody got in");
        assert_eq!(gate.open_count(), 0, "every permit was given back");
        assert!(gate.wait_idle(Duration::ZERO));
    });
}
