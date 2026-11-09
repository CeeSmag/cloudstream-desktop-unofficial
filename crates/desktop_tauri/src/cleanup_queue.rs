//! The cleanup queue: everything the app tidies up in the background runs here, one task at a time.
//!
//! Nothing runs while the app is still starting. The queue waits until start-up is finished (the
//! extensions are loaded), then a settling time, and then takes one task at a time with a pause between
//! them, so cleaning never competes with starting the app, with several other cleanups, or with a video
//! that is playing (while one plays or loads, the queue waits). Each task is logged with its time, and a
//! task that panics is logged and does not stop the queue.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// How long the queue waits after start-up has finished before the first task.
pub const SETTLE: Duration = Duration::from_secs(60);
/// The pause between two tasks.
pub const PAUSE: Duration = Duration::from_secs(3);
/// How long to wait before looking again when a video is playing or loading.
pub const BUSY_WAIT: Duration = Duration::from_secs(30);
/// How long to wait before looking again when no task is due.
pub const IDLE_WAIT: Duration = Duration::from_secs(60);
/// How often start-up is looked at while the app starts.
pub const READY_POLL: Duration = Duration::from_secs(2);

/// What the queue needs to know about the app.
pub trait World: Send {
    /// Whether the app has finished starting.
    fn ready(&self) -> bool;
    /// Whether something heavy is going on that cleaning should wait for (a video playing or loading).
    fn busy(&self) -> bool;
}

/// One cleanup: it runs once, or again and again after `every`.
pub struct Task {
    name: &'static str,
    every: Option<Duration>,
    due: Instant,
    run: Box<dyn FnMut() + Send>,
}

impl Task {
    /// A task that runs once.
    pub fn once(name: &'static str, run: impl FnMut() + Send + 'static) -> Self {
        Self {
            name,
            every: None,
            due: Instant::now(),
            run: Box::new(run),
        }
    }

    /// A task that runs, and then again `every` after each run.
    pub fn every(name: &'static str, every: Duration, run: impl FnMut() + Send + 'static) -> Self {
        Self {
            every: Some(every),
            ..Self::once(name, run)
        }
    }
}

/// The waiting tasks, in the order they were added.
#[derive(Default)]
pub struct Queue {
    tasks: Vec<Task>,
}

/// What the worker should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// The app is still starting.
    NotReady,
    /// The app has started; the settling time has not passed.
    Settling,
    /// A video is playing or loading.
    Busy,
    /// A task ran; pause before the next.
    Ran,
    /// Nothing is due.
    Idle,
    /// No tasks are left.
    Done,
}

impl Queue {
    pub fn add(&mut self, task: Task) {
        self.tasks.push(task);
    }

    fn len(&self) -> usize {
        self.tasks.len()
    }

    /// One step at `now`: runs the first due task if the app is ready, settled and not busy.
    /// `ready_at` remembers when the app was first seen ready.
    pub fn step(
        &mut self,
        world: &dyn World,
        now: Instant,
        ready_at: &mut Option<Instant>,
    ) -> Next {
        if self.tasks.is_empty() {
            return Next::Done;
        }
        if !world.ready() {
            return Next::NotReady;
        }
        let since = *ready_at.get_or_insert(now);
        if now.saturating_duration_since(since) < SETTLE {
            return Next::Settling;
        }
        if world.busy() {
            return Next::Busy;
        }
        match self.tasks.iter().position(|task| task.due <= now) {
            Some(index) => {
                self.run_at(index, now);
                Next::Ran
            }
            None => Next::Idle,
        }
    }

    fn run_at(&mut self, index: usize, now: Instant) {
        let Some(task) = self.tasks.get_mut(index) else {
            return;
        };
        let name = task.name;
        tracing::info!(task = name, "cleanup task started");
        let started = Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| (task.run)()));
        let took_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match outcome {
            Ok(()) => tracing::info!(task = name, took_ms, "cleanup task done"),
            Err(_) => tracing::error!(
                task = name,
                took_ms,
                "cleanup task PANICKED; it is skipped this time"
            ),
        }
        match task.every {
            Some(every) => task.due = now + every,
            None => {
                self.tasks.remove(index);
            }
        }
    }
}

/// Runs `queue` on its own thread until it is empty (a queue with repeating tasks never is).
pub fn spawn(mut queue: Queue, world: impl World + 'static) {
    tracing::info!(
        tasks = queue.len(),
        "cleanup queue created; it waits until the app has fully started"
    );
    let spawned = std::thread::Builder::new()
        .name("cleanup-queue".to_owned())
        .spawn(move || {
            let mut ready_at = None;
            let mut last = Next::NotReady;
            loop {
                let next = queue.step(&world, Instant::now(), &mut ready_at);
                if next != last {
                    tracing::info!(state = ?next, "cleanup queue");
                    last = next;
                }
                match next {
                    Next::Done => break,
                    Next::NotReady => std::thread::sleep(READY_POLL),
                    Next::Settling => std::thread::sleep(PAUSE),
                    Next::Busy => std::thread::sleep(BUSY_WAIT),
                    Next::Ran => std::thread::sleep(PAUSE),
                    Next::Idle => std::thread::sleep(IDLE_WAIT),
                }
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start the cleanup queue; nothing will be cleaned automatically");
    }
}

#[cfg(test)]
mod tests {
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
}
