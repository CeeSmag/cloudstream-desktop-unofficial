//! A handle the rest of the app uses to talk to the cleanup queue: ask it to run soon, or push
//! every repeating task's next run out by its interval after a manual cleanup.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// What the queue should do with task dues before the next step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pending {
    /// Make every repeating task due at once (then each run spaces itself by its interval).
    RunNow,
    /// After a manual cleanup: next run of each repeating task is `now + its interval`.
    ResetSchedule,
}

/// Shared signal between the cleanup worker and callers (Settings, home-after-ready).
#[derive(Clone)]
pub struct CleanupHandle {
    pending: Arc<Mutex<Option<Pending>>>,
    wake: Arc<(Mutex<()>, Condvar)>,
    alive: Arc<AtomicBool>,
}

impl CleanupHandle {
    pub(crate) fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(None)),
            wake: Arc::new((Mutex::new(()), Condvar::new())),
            alive: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn mark_alive(&self) {
        self.alive.store(true, Ordering::SeqCst);
    }

    pub(crate) fn mark_stopped(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }

    /// Ask the queue to run every repeating cleanup as soon as it is free.
    pub fn request_now(&self) {
        self.set(Pending::RunNow);
    }

    /// After a manual cleanup, restart the hourly clock from now.
    pub fn reset_schedule(&self) {
        self.set(Pending::ResetSchedule);
    }

    fn set(&self, next: Pending) {
        if !self.alive.load(Ordering::SeqCst) {
            tracing::warn!(
                ?next,
                "cleanup queue is not running yet; the request is kept until it starts"
            );
        }
        {
            let mut slot = self.pending.lock().unwrap_or_else(|p| p.into_inner());
            // RunNow wins over ResetSchedule: the queue will run and then space itself.
            *slot = Some(match (*slot, next) {
                (Some(Pending::RunNow), _) | (_, Pending::RunNow) => Pending::RunNow,
                _ => next,
            });
        }
        self.wake.1.notify_all();
        tracing::info!(?next, "cleanup queue signalled");
    }

    /// Takes the pending action for one step (clears it).
    pub(crate) fn take_pending(&self) -> Option<Pending> {
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }

    /// Sleeps up to `max`, waking early when someone signals.
    pub(crate) fn wait(&self, max: Duration) {
        let Ok(guard) = self.wake.0.lock() else {
            std::thread::sleep(max);
            return;
        };
        let _ = self.wake.1.wait_timeout(guard, max);
    }
}

#[cfg(test)]
#[path = "cleanup_handle_tests.rs"]
mod tests;
