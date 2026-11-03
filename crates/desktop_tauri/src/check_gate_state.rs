//! What the gate knows about the streams of the video that is being opened, and the rules that
//! turn it into a decision.

use std::time::{Duration, Instant};

use crate::check_progress::{CheckProgress, SavedSources, StartDecision};

/// Message of the failure when the search ended without finding any stream.
pub(crate) const NO_STREAMS: &str = "No playable stream links found";

/// The best of the saved streams are being tested; the first that works starts the video.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SavedTest {
    /// How many streams, from the first index on, are being tested.
    pub(crate) count: usize,
    /// When the test stops counting, whatever the answers.
    pub(crate) until: Instant,
}

pub(crate) struct GateState {
    pub(crate) progress: CheckProgress,
    /// Rank of every stream found, by index.
    pub(crate) ranks: Vec<i32>,
    /// Whether the check of each stream (by index) found it broken.
    pub(crate) broken: Vec<bool>,
    /// Whether each stream (by index) has been checked.
    pub(crate) answered: Vec<bool>,
    /// How many streams came from the provider's search (saved ones do not count).
    pub(crate) from_provider: usize,
    /// When the video was opened.
    pub(crate) opened_at: Instant,
    /// Best working stream so far: highest rank, then lowest index.
    pub(crate) best_working: Option<usize>,
    pub(crate) failure: Option<String>,
    pub(crate) cancelled: bool,
    /// Saved streams trusted as they are: play this one at once.
    pub(crate) start_on: Option<usize>,
    /// Saved streams being tested before playing.
    pub(crate) saved_test: Option<SavedTest>,
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
            answered: Vec::new(),
            from_provider: 0,
            opened_at: Instant::now(),
            best_working: None,
            failure: None,
            cancelled: false,
            start_on: None,
            saved_test: None,
        }
    }
}

impl GateState {
    fn rank(&self, index: usize) -> i32 {
        self.ranks.get(index).copied().unwrap_or_default()
    }

    /// Whether stream `index` is a better start than `other`.
    pub(crate) fn beats(&self, index: usize, other: usize) -> bool {
        let (rank, other_rank) = (self.rank(index), self.rank(other));
        rank > other_rank || (rank == other_rank && index < other)
    }

    /// Records newly found streams.
    pub(crate) fn add(&mut self, ranks: &[i32], from_provider: bool) {
        self.ranks.extend_from_slice(ranks);
        let known = self.ranks.len();
        self.broken.resize(known, false);
        self.answered.resize(known, false);
        self.progress.total = known;
        if from_provider {
            self.from_provider += ranks.len();
        }
    }

    /// Records the answer for the stream at `index`.
    pub(crate) fn answer(&mut self, index: usize, working: bool) {
        self.progress.checked += 1;
        if let Some(slot) = self.answered.get_mut(index) {
            *slot = true;
        }
        if !working {
            if let Some(slot) = self.broken.get_mut(index) {
                *slot = true;
            }
            return;
        }
        self.progress.working += 1;
        if self.best_working.is_none_or(|best| self.beats(index, best)) {
            self.best_working = Some(index);
        }
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
    pub(crate) fn limit_decision(&self, limit: Duration) -> StartDecision {
        if let Some(best) = self.best_working.or_else(|| self.best_unbroken()) {
            return StartDecision::Start(best);
        }
        if self.from_provider == 0 {
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
    pub(crate) fn finished_decision(&self) -> StartDecision {
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

    /// While saved streams are tested: the one to start on once one works. When all of the tested
    /// ones answered without a working one, or time ran out, the test is over and the video
    /// waits for the search like any other.
    pub(crate) fn saved_test_decision(&mut self) -> Option<StartDecision> {
        let test = self.saved_test?;
        let tested = test.count.min(self.ranks.len());
        let working = (0..tested)
            .filter(|index| self.answered.get(*index).copied().unwrap_or(false))
            .filter(|index| !self.broken.get(*index).copied().unwrap_or(true))
            .fold(None, |best: Option<usize>, index| match best {
                Some(current) if !self.beats(index, current) => Some(current),
                _ => Some(index),
            });
        if let Some(best) = working {
            self.saved_test = None;
            return Some(StartDecision::Start(best));
        }
        let all_answered =
            (0..tested).all(|index| self.answered.get(index).copied().unwrap_or(false));
        if all_answered || Instant::now() >= test.until {
            tracing::warn!(
                tested,
                all_answered,
                "no saved source works; searching again"
            );
            self.saved_test = None;
            self.progress.saved = SavedSources::OutOfDate;
        }
        None
    }
}

/// The error when the provider gave no stream at all in time.
pub(crate) fn silent_provider(limit: Duration) -> String {
    format!(
        "The provider did not answer within {} seconds. Check your connection or try again.",
        limit.as_secs()
    )
}
