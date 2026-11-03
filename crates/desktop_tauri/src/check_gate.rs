//! Lets the open video wait for the streams and decides when playback may begin.
//!
//! Streams arrive while the provider still searches and are checked as they come. Playback starts
//! on the best working stream a short grace after the first one is known (shorter when it is high
//! quality, since little better can follow), at once when the viewer skips, or when the search is
//! over and everything has been checked. Closing the video cancels the wait.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;

/// Qualities from this one up are worth starting on without waiting for better ones.
const HIGH_QUALITY: i32 = 1080;

/// Message of the failure when the search ended without finding any stream.
const NO_STREAMS: &str = "No playable stream links found";

/// How long to wait for a better stream after the first working one is known.
#[derive(Debug, Clone, Copy)]
pub struct Grace {
    /// When the best working stream so far is high quality.
    pub high_quality: Duration,
    /// Otherwise.
    pub other: Duration,
}

impl Default for Grace {
    fn default() -> Self {
        Self {
            high_quality: Duration::from_millis(900),
            other: Duration::from_millis(2000),
        }
    }
}

/// How far the search and the checks have come; sent to the UI.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct CheckProgress {
    /// Streams answered so far.
    pub checked: usize,
    /// Streams found so far (it grows while the provider is still searching).
    pub total: usize,
    /// Of the answered ones, how many can be played.
    pub working: usize,
    /// The viewer asked to start without waiting for the rest.
    pub skipping: bool,
    /// The provider is still looking for streams.
    pub discovering: bool,
}

/// What the wait for playback came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartDecision {
    /// Play the stream with this index.
    Start(usize),
    /// The video was closed meanwhile.
    Cancelled,
    /// There is nothing to play; the text says why.
    Failed(String),
}

struct GateState {
    progress: CheckProgress,
    /// Rank of every stream found, by index.
    ranks: Vec<i32>,
    /// Best working stream so far: highest rank, then lowest index.
    best_working: Option<usize>,
    /// When the first working stream was known.
    working_since: Option<Instant>,
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
            best_working: None,
            working_since: None,
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
    grace: Grace,
    changed: Condvar,
}

impl CheckGate {
    /// A gate that waits `grace` for better streams.
    pub fn new(grace: Grace) -> Self {
        Self {
            grace,
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
        if working {
            state.progress.working += 1;
            state.working_since.get_or_insert_with(Instant::now);
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
            Some(best) => self.after_grace(state, best),
            None => Verdict::Wait(None),
        }
    }

    fn after_grace(&self, state: &GateState, best: usize) -> Verdict {
        let grace = if state.rank(best) >= HIGH_QUALITY {
            self.grace.high_quality
        } else {
            self.grace.other
        };
        let waited = state
            .working_since
            .map_or(Duration::ZERO, |at| at.elapsed());
        match grace.checked_sub(waited) {
            Some(left) if !left.is_zero() => Verdict::Wait(Some(left)),
            _ => Verdict::Ready(StartDecision::Start(best)),
        }
    }
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
