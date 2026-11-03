//! Lets the open video wait for the stream checks and decides when playback may begin.
//!
//! Playback starts when every stream has been checked, or earlier when the viewer skips and a
//! working stream is already known. Closing the video cancels the wait.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use serde::Serialize;

/// How far the checks have come; sent to the UI.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct CheckProgress {
    /// Streams answered so far.
    pub checked: usize,
    /// Streams to check in all (0 while the streams are still being looked up).
    pub total: usize,
    /// Of the answered ones, how many can be played.
    pub working: usize,
    /// The viewer asked to start without waiting for the rest.
    pub skipping: bool,
}

#[derive(Default)]
struct GateState {
    progress: CheckProgress,
    /// Index of the best (lowest) working stream found so far.
    best_working: Option<usize>,
    finished: bool,
    cancelled: bool,
}

/// Shared between the check threads, the command that waits to play, and the skip button.
#[derive(Default)]
pub struct CheckGate {
    state: Mutex<GateState>,
    changed: Condvar,
}

impl CheckGate {
    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The latest progress.
    pub fn progress(&self) -> CheckProgress {
        self.lock().progress
    }

    /// Records how many streams will be checked.
    pub fn set_total(&self, total: usize) -> CheckProgress {
        let mut state = self.lock();
        state.progress.total = total;
        state.progress
    }

    /// Records the answer for the stream at `index`.
    pub fn record(&self, index: usize, working: bool) -> CheckProgress {
        let mut state = self.lock();
        state.progress.checked += 1;
        if working {
            state.progress.working += 1;
            state.best_working = Some(state.best_working.map_or(index, |best| best.min(index)));
        }
        let progress = state.progress;
        drop(state);
        self.changed.notify_all();
        progress
    }

    /// Records that every stream has been answered.
    pub fn finish(&self) {
        self.lock().finished = true;
        self.changed.notify_all();
    }

    /// The viewer wants to start now; playback begins as soon as a working stream is known.
    pub fn skip(&self) -> CheckProgress {
        let mut state = self.lock();
        state.progress.skipping = true;
        let progress = state.progress;
        drop(state);
        self.changed.notify_all();
        progress
    }

    /// The video was closed or replaced: stop waiting and stop checking.
    pub fn cancel(&self) {
        self.lock().cancelled = true;
        self.changed.notify_all();
    }

    /// Whether the video was closed or replaced.
    pub fn is_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    /// Blocks until playback may begin and returns the stream to start with: the best working
    /// one, or the first stream when none works (the player then reports what happens).
    /// `None` means the video was closed meanwhile.
    pub fn wait_for_start(&self) -> Option<usize> {
        let mut state = self.lock();
        loop {
            if state.cancelled {
                return None;
            }
            if state.finished || (state.progress.skipping && state.best_working.is_some()) {
                return Some(state.best_working.unwrap_or(0));
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    fn after_a_moment(action: impl FnOnce() + Send + 'static) {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            action();
        });
    }

    #[test]
    fn starts_on_the_best_working_stream_once_everything_is_checked() {
        let gate = Arc::new(CheckGate::default());
        gate.set_total(3);
        gate.record(2, true);
        gate.record(1, true);
        gate.record(0, false);
        gate.finish();
        assert_eq!(gate.wait_for_start(), Some(1));
    }

    #[test]
    fn starts_on_the_first_stream_when_none_works() {
        let gate = CheckGate::default();
        gate.record(0, false);
        gate.finish();
        assert_eq!(gate.wait_for_start(), Some(0));
    }

    #[test]
    fn waits_for_the_checks_when_nobody_skips() {
        let gate = Arc::new(CheckGate::default());
        gate.record(4, true);
        let later = Arc::clone(&gate);
        after_a_moment(move || {
            later.record(2, true);
            later.finish();
        });
        assert_eq!(gate.wait_for_start(), Some(2), "waited for the rest");
    }

    #[test]
    fn skipping_starts_at_once_on_what_is_known_and_leaves_the_checks_running() {
        let gate = CheckGate::default();
        gate.set_total(10);
        gate.record(3, true);
        gate.record(1, false);
        assert!(gate.skip().skipping);
        assert_eq!(gate.wait_for_start(), Some(3));
        assert!(!gate.is_cancelled());
    }

    #[test]
    fn skipping_waits_for_the_first_working_stream() {
        let gate = Arc::new(CheckGate::default());
        gate.record(0, false);
        gate.skip();
        let later = Arc::clone(&gate);
        after_a_moment(move || {
            later.record(5, true);
        });
        assert_eq!(gate.wait_for_start(), Some(5));
    }

    #[test]
    fn closing_the_video_ends_the_wait() {
        let gate = Arc::new(CheckGate::default());
        let later = Arc::clone(&gate);
        after_a_moment(move || later.cancel());
        assert_eq!(gate.wait_for_start(), None);
        assert!(gate.is_cancelled());
    }

    #[test]
    fn progress_counts_answers_and_working_streams() {
        let gate = CheckGate::default();
        gate.set_total(4);
        gate.record(0, true);
        let progress = gate.record(1, false);
        assert_eq!(
            progress,
            CheckProgress {
                checked: 2,
                total: 4,
                working: 1,
                skipping: false
            }
        );
    }
}
