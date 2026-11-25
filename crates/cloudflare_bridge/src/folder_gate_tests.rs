use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::pass_fixtures::folder;

use super::*;

/// Fails the test instead of hanging the run when a thread never finishes (a lost wake-up or a stuck turn).
const DEADLOCK_GUARD: Duration = Duration::from_secs(20);

fn join_all<T: Send + 'static>(workers: Vec<thread::JoinHandle<T>>) -> Vec<T> {
    let (done, finished) = mpsc::channel();
    let count = workers.len();
    thread::spawn(move || {
        for worker in workers {
            let _ = done.send(worker.join());
        }
    });
    (0..count)
        .map(|_| {
            finished
                .recv_timeout(DEADLOCK_GUARD)
                .expect("every entrant finishes")
                .expect("no entrant panicked")
        })
        .collect()
}

/// The order that used to go wrong with polling: an entrant is waiting, the holder leaves and a later
/// arrival asks at that very moment. Here the later ticket asks first and still gets in second.
#[test]
fn turns_follow_ticket_order_even_when_later_tickets_ask_first() {
    let gate = Arc::new(FolderGate::default());
    let served = Arc::new(Mutex::new(Vec::new()));
    let holding = gate.enter();
    let (earlier, later) = (gate.take_ticket(), gate.take_ticket());
    let waiters: Vec<_> = [later, earlier]
        .into_iter()
        .map(|ticket| {
            let (gate, served) = (Arc::clone(&gate), Arc::clone(&served));
            thread::spawn(move || {
                let _turn = gate.wait_for_turn(ticket);
                served.lock().expect("served").push(ticket);
            })
        })
        .collect();
    while gate.queued() != 3 {
        thread::yield_now(); // both waiters hold their tickets already; nothing to wait for but the turn
    }
    drop(holding);
    join_all(waiters);
    assert_eq!(*served.lock().expect("served"), [earlier, later]);
    assert_eq!(gate.queued(), 0);
}

/// Stress: entrants that come straight back (like the pass book's readers) are served one at a time and
/// strictly in ticket order, so none can be overtaken however the threads are scheduled.
#[test]
fn concurrent_entrants_are_served_one_at_a_time_in_ticket_order() {
    const ENTRANTS: usize = 6;
    const ROUNDS: usize = 500;
    let gate = Arc::new(FolderGate::default());
    let inside = Arc::new(AtomicUsize::new(0));
    let served = Arc::new(Mutex::new(Vec::with_capacity(ENTRANTS * ROUNDS)));
    let workers = (0..ENTRANTS)
        .map(|_| {
            let (gate, inside, served) =
                (Arc::clone(&gate), Arc::clone(&inside), Arc::clone(&served));
            thread::spawn(move || {
                for _ in 0..ROUNDS {
                    let ticket = gate.take_ticket();
                    let _turn = gate.wait_for_turn(ticket);
                    assert_eq!(inside.fetch_add(1, Ordering::SeqCst), 0, "two holders");
                    served.lock().expect("served").push(ticket);
                    inside.fetch_sub(1, Ordering::SeqCst);
                }
            })
        })
        .collect();
    join_all(workers);
    let served = served.lock().expect("served");
    let expected: Vec<u64> = (0..u64::try_from(ENTRANTS * ROUNDS).expect("fits")).collect();
    assert_eq!(*served, expected, "served exactly in ticket order");
    assert_eq!(gate.queued(), 0);
}

#[test]
fn a_holder_that_panics_passes_the_turn_on() {
    let gate = Arc::new(FolderGate::default());
    let crashing = Arc::clone(&gate);
    let crashed = thread::spawn(move || {
        let _turn = crashing.enter();
        std::panic::resume_unwind(Box::new("holder crashed"));
    })
    .join();
    assert!(crashed.is_err());
    drop(gate.enter());
    assert_eq!(gate.queued(), 0);
}

#[test]
fn books_on_one_folder_share_one_queue_and_other_folders_do_not() {
    let (one, other) = (folder("gate_one"), folder("gate_other"));
    let first = FolderGate::for_folder(&one);
    assert!(Arc::ptr_eq(&first, &FolderGate::for_folder(&one)));
    assert!(Arc::ptr_eq(&first, &FolderGate::for_folder(&one.join("."))));
    assert!(!Arc::ptr_eq(&first, &FolderGate::for_folder(&other)));
    let missing = one.join("not-created");
    assert!(Arc::ptr_eq(
        &FolderGate::for_folder(&missing),
        &FolderGate::for_folder(&missing)
    ));
    let _ = fs::remove_dir_all(&one);
    let _ = fs::remove_dir_all(&other);
}

#[test]
fn a_folder_no_book_uses_any_more_gets_a_fresh_queue() {
    let dir = folder("gate_forgotten");
    let first = FolderGate::for_folder(&dir);
    let _abandoned = first.take_ticket();
    assert_eq!(first.queued(), 1);
    drop(first);
    assert_eq!(FolderGate::for_folder(&dir).queued(), 0);
    let _ = fs::remove_dir_all(&dir);
}
