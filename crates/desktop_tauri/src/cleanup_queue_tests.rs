//! Tests of the cleanup queue: start-up and settling, one task per step in order, waiting while a
//! video plays, repeating tasks, a panicking task, and handle-driven run-now / reset-schedule.

use super::*;
use crate::cleanup_handle::CleanupHandle;
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

fn hourly(count: &Arc<AtomicUsize>) -> Task {
    let tally = Arc::clone(count);
    Task::every("hourly", Duration::from_secs(3600), move || {
        tally.fetch_add(1, Ordering::SeqCst);
    })
}

/// Bundles the parts of a step that stay the same across several ticks in a test.
struct Tick<'a> {
    world: &'a dyn World,
    ready_at: &'a mut Option<Instant>,
    handle: Option<&'a CleanupHandle>,
}

fn at(queue: &mut Queue, tick: &mut Tick<'_>, now: Instant) -> Next {
    queue.step(Step {
        world: tick.world,
        now,
        ready_at: tick.ready_at,
        handle: tick.handle,
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
    let mut tick = Tick {
        world: &world,
        ready_at: &mut ready_at,
        handle: None,
    };
    assert_eq!(at(&mut queue, &mut tick, start), Next::NotReady);
    world.ready.store(true, Ordering::SeqCst);
    assert_eq!(at(&mut queue, &mut tick, start), Next::Settling);
    assert_eq!(
        at(&mut queue, &mut tick, start + SETTLE / 2),
        Next::Settling
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(at(&mut queue, &mut tick, start + SETTLE), Next::Ran);
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
    let mut tick = Tick {
        world: &world,
        ready_at: &mut ready_at,
        handle: None,
    };
    at(&mut queue, &mut tick, start);
    assert_eq!(at(&mut queue, &mut tick, now), Next::Ran);
    assert_eq!(order.lock().expect("order").len(), 1);
    assert_eq!(at(&mut queue, &mut tick, now), Next::Ran);
    assert_eq!(at(&mut queue, &mut tick, now), Next::Ran);
    assert_eq!(
        *order.lock().expect("order"),
        vec!["first", "second", "third"]
    );
    assert_eq!(at(&mut queue, &mut tick, now), Next::Done);
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
    let mut tick = Tick {
        world: &world,
        ready_at: &mut ready_at,
        handle: None,
    };
    assert_eq!(at(&mut queue, &mut tick, now), Next::Busy);
    world.busy.store(false, Ordering::SeqCst);
    assert_eq!(at(&mut queue, &mut tick, now), Next::Ran);
}

#[test]
fn a_repeating_task_comes_back_after_its_interval_and_not_before() {
    let (world, count, start) = (
        Fake::new(true, false),
        Arc::new(AtomicUsize::new(0)),
        Instant::now(),
    );
    let mut queue = Queue::default();
    queue.add(hourly(&count));
    let mut ready_at = Some(start);
    let now = start + SETTLE;
    let mut tick = Tick {
        world: &world,
        ready_at: &mut ready_at,
        handle: None,
    };
    assert_eq!(at(&mut queue, &mut tick, now), Next::Ran);
    assert_eq!(
        at(&mut queue, &mut tick, now + Duration::from_secs(1800)),
        Next::Idle
    );
    assert_eq!(
        at(&mut queue, &mut tick, now + Duration::from_secs(3601)),
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
    let mut tick = Tick {
        world: &world,
        ready_at: &mut ready_at,
        handle: None,
    };
    assert_eq!(at(&mut queue, &mut tick, now), Next::Ran);
    assert_eq!(at(&mut queue, &mut tick, now), Next::Ran);
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn request_now_makes_a_repeating_task_due_then_spaces_it_again() {
    let (world, count, start) = (
        Fake::new(true, false),
        Arc::new(AtomicUsize::new(0)),
        Instant::now(),
    );
    let mut queue = Queue::default();
    queue.add(hourly(&count));
    let handle = CleanupHandle::new();
    handle.mark_alive();
    let mut ready_at = Some(start);
    let now = start + SETTLE;
    let mut tick = Tick {
        world: &world,
        ready_at: &mut ready_at,
        handle: Some(&handle),
    };
    assert_eq!(at(&mut queue, &mut tick, now), Next::Ran);
    let later = now + Duration::from_secs(10);
    assert_eq!(at(&mut queue, &mut tick, later), Next::Idle);
    handle.request_now();
    assert_eq!(at(&mut queue, &mut tick, later), Next::Ran);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert_eq!(
        at(&mut queue, &mut tick, later + Duration::from_secs(10)),
        Next::Idle
    );
}

#[test]
fn reset_schedule_pushes_the_next_run_out_by_the_interval() {
    let (world, count, start) = (
        Fake::new(true, false),
        Arc::new(AtomicUsize::new(0)),
        Instant::now(),
    );
    let mut queue = Queue::default();
    queue.add(hourly(&count));
    let handle = CleanupHandle::new();
    handle.mark_alive();
    let mut ready_at = Some(start);
    let now = start + SETTLE;
    let mut tick = Tick {
        world: &world,
        ready_at: &mut ready_at,
        handle: Some(&handle),
    };
    handle.reset_schedule();
    assert_eq!(at(&mut queue, &mut tick, now), Next::Idle);
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(
        at(&mut queue, &mut tick, now + Duration::from_secs(3601)),
        Next::Ran
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
