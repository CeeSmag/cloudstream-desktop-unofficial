//! Lets the open video wait for the streams and decides when playback may begin.
//!
//! Streams arrive while the provider still searches and are checked as they come. Playback starts
//! on the best working stream only when the search is over and every stream has been checked, or
//! at once when the viewer skips (and a stream is known to work). The waiting has limits: if the
//! provider gives no stream at all within [`WaitLimits::first_stream`] the wait fails with an
//! error, and whatever happens the wait ends [`WaitLimits::max_wait`] after the video was opened:
//! the best working stream plays, or else the best one not known to be broken, or else the wait
//! fails. Closing the video cancels the wait.
//!
//! Sources saved from an earlier time are the one exception to the wait: [`CheckGate::trust_saved`]
//! plays the saved stream at once, and [`CheckGate::test_saved_first`] plays the first of the best
//! saved streams that checks out, giving up on them after a short time.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::check_gate_state::{silent_provider, GateState, SavedTest};
pub use crate::check_progress::{CheckProgress, SavedSources, StartDecision, WaitLimits};

enum Verdict {
    Ready(StartDecision),
    /// Wait for news, at most this long.
    Wait(Option<Duration>),
}

/// Shared between the search and check threads, the command that waits to play, and the skip button.
#[derive(Default)]
pub struct CheckGate {
    state: Mutex<GateState>,
    limits: WaitLimits,
    changed: Condvar,
}

impl CheckGate {
    /// A gate that gives the provider the time `limits` allow.
    pub fn new(limits: WaitLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Changes the state, wakes the waiter and returns the progress.
    fn update(&self, change: impl FnOnce(&mut GateState)) -> CheckProgress {
        let mut state = self.lock();
        change(&mut state);
        let progress = state.progress;
        drop(state);
        self.changed.notify_all();
        progress
    }

    /// The latest progress.
    pub fn progress(&self) -> CheckProgress {
        self.lock().progress
    }

    /// Records newly found streams; `ranks` are theirs, in the order of their indexes.
    pub fn announce(&self, ranks: &[i32]) -> CheckProgress {
        self.update(|state| state.add(ranks, true))
    }

    /// Records streams that were saved from an earlier time, which the provider has not found
    /// (yet). They do not count as an answer of the provider.
    pub fn announce_saved(&self, ranks: &[i32]) -> CheckProgress {
        self.update(|state| state.add(ranks, false))
    }

    /// Records the answer for the stream at `index`.
    pub fn record(&self, index: usize, working: bool) -> CheckProgress {
        self.update(|state| state.answer(index, working))
    }

    /// The search is over; `failure` says why when it ended because the provider failed.
    pub fn discovery_ended(&self, failure: Option<String>) -> CheckProgress {
        self.update(|state| {
            state.progress.discovering = false;
            state.failure = failure;
        })
    }

    /// The viewer wants to start now; playback begins as soon as a working stream is known. The
    /// search and the checks go on.
    pub fn skip(&self) -> CheckProgress {
        self.update(|state| state.progress.skipping = true)
    }

    /// The saved stream with this index was saved so recently that it is played at once.
    pub fn trust_saved(&self, index: usize) -> CheckProgress {
        self.update(|state| {
            state.start_on = Some(index);
            state.progress.saved = SavedSources::Trusted;
        })
    }

    /// The first `count` streams were saved a while ago: the first of them that checks out starts
    /// the video. If none does within `cap`, the video waits for the search like any other.
    pub fn test_saved_first(&self, count: usize, cap: Duration) -> CheckProgress {
        self.update(|state| {
            state.saved_test = Some(SavedTest {
                count,
                until: Instant::now() + cap,
            });
            state.progress.saved = SavedSources::Testing;
        })
    }

    /// The video was closed or replaced: stop waiting and stop checking.
    pub fn cancel(&self) {
        self.update(|state| state.cancelled = true);
    }

    /// Whether the video was closed or replaced.
    pub fn is_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    /// Blocks until playback may begin and says which stream to start with: the best working
    /// one, or the best one found when none works (the player then reports what happens).
    pub fn wait_for_start(&self) -> StartDecision {
        let mut state = self.lock();
        loop {
            state = match self.verdict(&mut state) {
                Verdict::Ready(decision) => return decision,
                Verdict::Wait(None) => self
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner),
                Verdict::Wait(Some(left)) => {
                    self.changed
                        .wait_timeout(state, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }

    fn verdict(&self, state: &mut GateState) -> Verdict {
        if state.cancelled {
            return Verdict::Ready(StartDecision::Cancelled);
        }
        if let Some(index) = state.start_on {
            return Verdict::Ready(StartDecision::Start(index));
        }
        let progress = state.progress;
        if !progress.discovering && progress.checked >= progress.total {
            return Verdict::Ready(state.finished_decision());
        }
        match state.best_working {
            Some(best) if progress.skipping => return Verdict::Ready(StartDecision::Start(best)),
            _ => {}
        }
        if let Some(decision) = state.saved_test_decision() {
            return Verdict::Ready(decision);
        }
        sooner(self.by_the_clock(state), state)
    }

    /// Nothing is decided yet: wait for news until the next limit, and enforce a limit that passed.
    fn by_the_clock(&self, state: &GateState) -> Verdict {
        let (first, max) = (self.limits.first_stream, self.limits.max_wait);
        let waited = state.opened_at.elapsed();
        if waited >= max {
            tracing::warn!(?waited, "the wait for the streams hit its limit");
            return Verdict::Ready(state.limit_decision(max));
        }
        if state.from_provider == 0 && state.saved_test.is_none() {
            if waited >= first {
                tracing::warn!(?waited, "the provider gave no stream in time");
                return Verdict::Ready(StartDecision::Failed(silent_provider(first)));
            }
            return Verdict::Wait(Some(first.min(max) - waited));
        }
        Verdict::Wait(Some(max - waited))
    }
}

/// While saved streams are tested the wait must also end when the test's time is up.
fn sooner(verdict: Verdict, state: &GateState) -> Verdict {
    let (Verdict::Wait(wait), Some(test)) = (&verdict, state.saved_test) else {
        return verdict;
    };
    let test_left = test.until.saturating_duration_since(Instant::now());
    Verdict::Wait(Some(wait.map_or(test_left, |wait| wait.min(test_left))))
}

#[cfg(test)]
#[path = "check_gate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "check_gate_saved_tests.rs"]
mod saved_tests;
