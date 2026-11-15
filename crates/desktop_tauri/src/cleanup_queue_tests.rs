//! Tests of the cleanup queue: start-up and settling, one task per step in order, waiting while a
//! video plays, repeating tasks, and a panicking task.

use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Fake {
    ready: AtomicBool,
    busy: AtomicBool,
}

impl Fake {
    fn new(ready: bool, busy: bool) -> Arc<Self> {
        Arc::new(Self {
            ready: AtomicBool::new(ready),
            busy: AtomicBool::new(busy),
        })
    }
}

impl World for Arc<Fake> {
    fn ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
}

fn counting(name: &'static str, count: &Arc<AtomicUsize>) -> Task {
    let count = Arc::clone(count);
    Task::once(name, move || {
        count.fetch_add(1, Ordering::SeqCst);
    })
}

#[test]
fn nothing_runs_while_the_app_is_starting_or_settling() {
    let (world, count, start) = (
        Fake::new(false, false),
        Arc::new(AtomicUsize::new(0)),
        Instant::now(),
    );
    let mut queue = Queue::default();
    queue.add(counting("a", &count));
    let mut ready_at = None;
    assert_eq!(queue.step(&world, start, &mut ready_at), Next::NotReady);
    world.ready.store(true, Ordering::SeqCst);
    assert_eq!(queue.step(&world, start, &mut ready_at), Next::Settling);
    assert_eq!(
        queue.step(&world, start + SETTLE / 2, &mut ready_at),
        Next::Settling
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "a task ran before the app had settled"
    );
    assert_eq!(queue.step(&world, start + SETTLE, &mut ready_at), Next::Ran);
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn one_task_per_step_in_the_order_added_and_a_once_task_is_gone_afterwards() {
    let (world, order) = (Fake::new(true, false), Arc::new(Mutex::new(Vec::new())));
    let mut queue = Queue::default();
    for name in ["first", "second", "third"] {
        let order = Arc::clone(&order);
        queue.add(Task::once(name, move || {
            order.lock().expect("order").push(name)
        }));
    }
    let (start, mut ready_at) = (Instant::now(), None);
    let now = start + SETTLE;
    queue.step(&world, start, &mut ready_at);
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Ran);
    assert_eq!(
        order.lock().expect("order").len(),
        1,
        "only one task per step"
    );
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Ran);
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Ran);
    assert_eq!(
        *order.lock().expect("order"),
        vec!["first", "second", "third"]
    );
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Done);
}

#[test]
fn a_video_playing_holds_the_queue_back() {
    let (world, count, start) = (
        Fake::new(true, true),
        Arc::new(AtomicUsize::new(0)),
        Instant::now(),
    );
    let mut queue = Queue::default();
    queue.add(counting("a", &count));
    let mut ready_at = Some(start);
    let now = start + SETTLE;
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Busy);
    assert_eq!(count.load(Ordering::SeqCst), 0);
    world.busy.store(false, Ordering::SeqCst);
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Ran);
}

#[test]
fn a_repeating_task_comes_back_after_its_interval_and_not_before() {
    let (world, count, start) = (
        Fake::new(true, false),
        Arc::new(AtomicUsize::new(0)),
        Instant::now(),
    );
    let tally = Arc::clone(&count);
    let mut queue = Queue::default();
    queue.add(Task::every(
        "hourly",
        Duration::from_secs(3600),
        move || {
            tally.fetch_add(1, Ordering::SeqCst);
        },
    ));
    let mut ready_at = Some(start);
    let now = start + SETTLE;
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Ran);
    assert_eq!(
        queue.step(&world, now + Duration::from_secs(1800), &mut ready_at),
        Next::Idle
    );
    assert_eq!(
        queue.step(&world, now + Duration::from_secs(3601), &mut ready_at),
        Next::Ran
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn a_task_that_panics_does_not_stop_the_ones_after_it() {
    let (world, count, start) = (
        Fake::new(true, false),
        Arc::new(AtomicUsize::new(0)),
        Instant::now(),
    );
    let mut queue = Queue::default();
    queue.add(Task::once("bad", || panic!("on purpose")));
    queue.add(counting("good", &count));
    let mut ready_at = Some(start);
    let now = start + SETTLE;
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Ran);
    assert_eq!(queue.step(&world, now, &mut ready_at), Next::Ran);
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
