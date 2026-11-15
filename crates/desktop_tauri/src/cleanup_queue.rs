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
#[path = "cleanup_queue_tests.rs"]
mod tests;
