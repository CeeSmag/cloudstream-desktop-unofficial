//! Priority gate for background HTTP stream checks: yields to subtitle loading after "Play now"
//! (with an 8-second grace cap) or when the viewer explicitly toggles Focus on Subtitles.

use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Upper bound on automatic post–"Play now" stream-check pausing when preferred subtitles do not
/// arrive (e.g. titles with embedded MKV subtitles only or no external subtitles).
pub const SUBTITLE_PRIORITY_GRACE: Duration = Duration::from_secs(8);

/// Priority mode controlling whether HTTP stream check workers run or yield to subtitles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CheckPriority {
    /// Run HTTP stream checks at full speed (default before playback, or Focus on Sources).
    #[default]
    Normal,
    /// Automatic pause after "Play now": wait until preferred subtitles are satisfied, the grace
    /// window expires, provider search finishes, or playback needs a fallback.
    SubtitlesAuto {
        /// Moment automatic subtitle priority began.
        started_at: Instant,
    },
    /// Viewer explicitly toggled `Focus` on Subtitles: hold checks paused until search finishes,
    /// the viewer disables Focus (or focuses Sources), or playback fails and needs a fallback.
    SubtitlesPinned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateDecision {
    Run,
    Pause(Option<Duration>),
}

/// Pure rule deciding whether stream checks may run right now or how long to wait.
pub(crate) fn evaluate_gate(
    priority: CheckPriority,
    search_done: bool,
    now: Instant,
    grace: Duration,
) -> GateDecision {
    if search_done {
        return GateDecision::Run;
    }
    match priority {
        CheckPriority::Normal => GateDecision::Run,
        CheckPriority::SubtitlesPinned => GateDecision::Pause(None),
        CheckPriority::SubtitlesAuto { started_at } => {
            let elapsed = now.saturating_duration_since(started_at);
            match grace.checked_sub(elapsed) {
                Some(remaining) if !remaining.is_zero() => GateDecision::Pause(Some(remaining)),
                _ => GateDecision::Run,
            }
        }
    }
}

#[derive(Default)]
struct GateState {
    priority: CheckPriority,
    search_done: bool,
    cancelled: bool,
}

/// Thread-safe condition gate owned by [`crate::OpenControl`] and waited on by check workers.
#[derive(Default)]
pub(crate) struct CheckGate {
    state: Mutex<GateState>,
    wake: Condvar,
}

impl CheckGate {
    /// Enters automatic subtitle-first mode unless already pinned by the user.
    pub(crate) fn prioritize_subtitles_auto(&self) {
        let mut guard = self.lock();
        if matches!(guard.priority, CheckPriority::Normal) {
            guard.priority = CheckPriority::SubtitlesAuto {
                started_at: Instant::now(),
            };
            tracing::debug!("stream check gate paused automatically for subtitles");
        }
        drop(guard);
        self.wake.notify_all();
    }

    /// Pins subtitle focus manually (no grace timeout; still wakes on search end or fallback).
    pub(crate) fn pin_subtitles_focus(&self) {
        let mut guard = self.lock();
        guard.priority = CheckPriority::SubtitlesPinned;
        drop(guard);
        tracing::debug!("stream check gate pinned to subtitles focus");
        self.wake.notify_all();
    }

    /// Leaves automatic subtitle-first mode once preferred subtitles are ready.
    /// Does not override a manual [`CheckPriority::SubtitlesPinned`].
    pub(crate) fn signal_subtitles_satisfied(&self) {
        let mut guard = self.lock();
        if matches!(guard.priority, CheckPriority::SubtitlesAuto { .. }) {
            guard.priority = CheckPriority::Normal;
            tracing::debug!("preferred subtitles ready; resuming background stream checks");
            drop(guard);
            self.wake.notify_all();
        }
    }

    /// Unconditionally resumes stream checks (Focus Sources, Focus off, or playback fallback).
    pub(crate) fn resume_checks_now(&self) {
        let mut guard = self.lock();
        guard.priority = CheckPriority::Normal;
        drop(guard);
        self.wake.notify_all();
    }

    /// Marks provider discovery finished so paused workers immediately resume checking.
    pub(crate) fn mark_search_done(&self) {
        let mut guard = self.lock();
        guard.search_done = true;
        drop(guard);
        self.wake.notify_all();
    }

    /// Cancels the open, waking all parked workers to exit immediately.
    pub(crate) fn cancel(&self) {
        let mut guard = self.lock();
        guard.cancelled = true;
        drop(guard);
        self.wake.notify_all();
    }

    /// Resets transient search/cancel flags when starting a fresh refresh pass.
    pub(crate) fn reset(&self) {
        let mut guard = self.lock();
        guard.priority = CheckPriority::Normal;
        guard.search_done = false;
        guard.cancelled = false;
        drop(guard);
        self.wake.notify_all();
    }

    /// Current priority mode (for diagnostics and tests).
    pub(crate) fn mode(&self) -> CheckPriority {
        self.lock().priority
    }

    /// Blocks while paused; returns `true` when permitted to run a check or `false` if cancelled.
    pub(crate) fn wait_until_runnable(&self, grace: Duration) -> bool {
        let mut guard = self.lock();
        loop {
            if guard.cancelled {
                return false;
            }
            match evaluate_gate(guard.priority, guard.search_done, Instant::now(), grace) {
                GateDecision::Run => {
                    if matches!(guard.priority, CheckPriority::SubtitlesAuto { .. }) {
                        guard.priority = CheckPriority::Normal;
                    }
                    return true;
                }
                GateDecision::Pause(Some(wait_for)) => {
                    let (next, _) = self
                        .wake
                        .wait_timeout(guard, wait_for)
                        .unwrap_or_else(PoisonError::into_inner);
                    guard = next;
                }
                GateDecision::Pause(None) => {
                    guard = self
                        .wake
                        .wait(guard)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "subtitle_priority_tests.rs"]
mod tests;
