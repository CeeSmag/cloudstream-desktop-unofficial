//! Lets the open video wait for the streams and decides when playback may begin.
//!
//! Streams arrive while the provider still searches and are checked as they come. Playback starts
//! on the best working stream only when the search is over and every stream has been checked, or
//! at once when the viewer skips (and a stream is known to work). The waiting has limits: if the
//! provider gives no stream at all within [`WaitLimits::first_stream`] the wait fails with an
//! error, and whatever happens the wait ends [`WaitLimits::max_wait`] after the video was opened:
//! the best working stream plays, or else the best one not known to be broken, or else the wait
//! fails. Closing the video cancels the wait.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Message of the failure when the search ended without finding any stream.
const NO_STREAMS: &str = "No playable stream links found";

pub use crate::check_progress::{CheckProgress, StartDecision, WaitLimits};

struct GateState {
    progress: CheckProgress,
    /// Rank of every stream found, by index.
    ranks: Vec<i32>,
    /// Whether the check of each stream (by index) found it broken.
    broken: Vec<bool>,
    /// When the video was opened.
    opened_at: Instant,
    /// Best working stream so far: highest rank, then lowest index.
    best_working: Option<usize>,
    failure: Option<String>,
    cancelled: bool,
}

impl Default for GateState {
    fn default() -> Self {
        Self {
            progress: CheckProgress {
                discovering: true,
                ..CheckProgress::default()
            },
            ranks: Vec::new(),
            broken: Vec::new(),
            opened_at: Instant::now(),
            best_working: None,
            failure: None,
            cancelled: false,
        }
    }
}

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

    /// The latest progress.
    pub fn progress(&self) -> CheckProgress {
        self.lock().progress
    }

    /// Records newly found streams; `ranks` are theirs, in the order of their indexes.
    pub fn announce(&self, ranks: &[i32]) -> CheckProgress {
        let mut state = self.lock();
        state.ranks.extend_from_slice(ranks);
        let known = state.ranks.len();
        state.broken.resize(known, false);
        state.progress.total = state.ranks.len();
        let progress = state.progress;
        drop(state);
        self.changed.notify_all();
        progress
    }

    /// Records the answer for the stream at `index`.
    pub fn record(&self, index: usize, working: bool) -> CheckProgress {
        let mut state = self.lock();
        state.progress.checked += 1;
        if !working {
            if let Some(slot) = state.broken.get_mut(index) {
                *slot = true;
            }
        }
        if working {
            state.progress.working += 1;
            if state
                .best_working
                .is_none_or(|best| state.beats(index, best))
            {
                state.best_working = Some(index);
            }
        }
        let progress = state.progress;
        drop(state);
        self.changed.notify_all();
        progress
    }

    /// The search is over; `failure` says why when it ended because the provider failed.
    pub fn discovery_ended(&self, failure: Option<String>) -> CheckProgress {
        let mut state = self.lock();
        state.progress.discovering = false;
        state.failure = failure;
        let progress = state.progress;
        drop(state);
        self.changed.notify_all();
        progress
    }

    /// The viewer wants to start now; playback begins as soon as a working stream is known. The
    /// search and the checks go on.
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

    /// Blocks until playback may begin and says which stream to start with: the best working
    /// one, or the best one found when none works (the player then reports what happens).
    pub fn wait_for_start(&self) -> StartDecision {
        let mut state = self.lock();
        loop {
            state = match self.verdict(&state) {
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

    fn verdict(&self, state: &GateState) -> Verdict {
        if state.cancelled {
            return Verdict::Ready(StartDecision::Cancelled);
        }
        let progress = state.progress;
        if !progress.discovering && progress.checked >= progress.total {
            return Verdict::Ready(state.finished_decision());
        }
        match state.best_working {
            Some(best) if progress.skipping => Verdict::Ready(StartDecision::Start(best)),
            _ => self.by_the_clock(state),
        }
    }

    /// Nothing is decided yet: wait for news until the next limit, and enforce a limit that passed.
    fn by_the_clock(&self, state: &GateState) -> Verdict {
        let (first, max) = (self.limits.first_stream, self.limits.max_wait);
        let waited = state.opened_at.elapsed();
        if waited >= max {
            tracing::warn!(?waited, "the wait for the streams hit its limit");
            return Verdict::Ready(state.limit_decision(max));
        }
        if state.ranks.is_empty() {
            if waited >= first {
                tracing::warn!(?waited, "the provider gave no stream in time");
                return Verdict::Ready(StartDecision::Failed(silent_provider(first)));
            }
            return Verdict::Wait(Some(first.min(max) - waited));
        }
        Verdict::Wait(Some(max - waited))
    }
}

/// The error when the provider gave no stream at all in time.
fn silent_provider(limit: Duration) -> String {
    format!(
        "The provider did not answer within {} seconds. Check your connection or try again.",
        limit.as_secs()
    )
}

impl GateState {
    fn rank(&self, index: usize) -> i32 {
        self.ranks.get(index).copied().unwrap_or_default()
    }

    /// Whether stream `index` is a better start than `other`.
    fn beats(&self, index: usize, other: usize) -> bool {
        let (rank, other_rank) = (self.rank(index), self.rank(other));
        rank > other_rank || (rank == other_rank && index < other)
    }

    /// The best stream not known to be broken.
    fn best_unbroken(&self) -> Option<usize> {
        (0..self.ranks.len())
            .filter(|index| !self.broken.get(*index).copied().unwrap_or(false))
            .fold(None, |best: Option<usize>, index| match best {
                Some(current) if !self.beats(index, current) => Some(current),
                _ => Some(index),
            })
    }

    /// The decision when the whole wait ran out: the best working stream, else the best one not
    /// known to be broken, else a failure that says what was tried.
    fn limit_decision(&self, limit: Duration) -> StartDecision {
        if let Some(best) = self.best_working.or_else(|| self.best_unbroken()) {
            return StartDecision::Start(best);
        }
        if self.ranks.is_empty() {
            return StartDecision::Failed(silent_provider(limit));
        }
        StartDecision::Failed(format!(
            "No working source was found within {} seconds ({} found, {} checked).",
            limit.as_secs(),
            self.progress.total,
            self.progress.checked
        ))
    }

    /// The decision once the search is over and every stream has been answered.
    fn finished_decision(&self) -> StartDecision {
        if let Some(best) = self.best_working {
            return StartDecision::Start(best);
        }
        let best_known =
            (0..self.ranks.len()).fold(None, |best: Option<usize>, index| match best {
                Some(current) if !self.beats(index, current) => Some(current),
                _ => Some(index),
            });
        match best_known {
            Some(index) => StartDecision::Start(index),
            None => StartDecision::Failed(
                self.failure
                    .clone()
                    .unwrap_or_else(|| NO_STREAMS.to_owned()),
            ),
        }
    }
}

#[cfg(test)]
#[path = "check_gate_tests.rs"]
mod tests;
