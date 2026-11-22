//! Which verified browser windows to release (plan step H1, `docs/MEMORY.md`).
//!
//! Every window is a browser process of its own (about 150 MiB each, measured), and a window that
//! passed used to stay until the helper quit. Only a site whose requests go through the browser
//! (`fetch-<id>.txt`) needs its window; the others keep one only as a fallback. A released window
//! costs the sidecar one re-check the next time it is needed, a path it already handles.
//!
//! Pure: the caller says how long each window has been unused, so no clock is needed here.

use std::time::Duration;

/// What decides a release.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextLimits {
    max_windows: usize,
    idle_ttl: Duration,
    grace: Duration,
}

impl ContextLimits {
    /// A window unused for `idle_ttl` is released; above `max_windows` the least recently used
    /// go first, but never one used within `grace` (so a burst of sites does not thrash).
    /// At least one window is always allowed.
    pub(crate) fn new(max_windows: usize, idle_ttl: Duration, grace: Duration) -> Self {
        Self {
            max_windows: max_windows.max(1),
            idle_ttl,
            grace,
        }
    }
}

/// One live window.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextState<'a> {
    /// The window's key in the helper.
    pub(crate) key: &'a str,
    /// The check passed (a window still loading or being checked is never released).
    pub(crate) passed: bool,
    /// A browser fetch is running in it.
    pub(crate) fetching: bool,
    /// Time since it was last used (passed, or a fetch started).
    pub(crate) idle: Duration,
}

impl ContextState<'_> {
    fn releasable(&self) -> bool {
        self.passed && !self.fetching
    }
}

/// The windows to release now.
pub(crate) fn contexts_to_release(
    contexts: &[ContextState<'_>],
    limits: &ContextLimits,
) -> Vec<String> {
    let expired = |state: &ContextState<'_>| state.releasable() && state.idle >= limits.idle_ttl;
    let mut release: Vec<String> = contexts
        .iter()
        .filter(|state| expired(state))
        .map(|state| state.key.to_owned())
        .collect();
    let kept = contexts
        .iter()
        .filter(|state| state.passed && !expired(state))
        .count();
    let excess = kept.saturating_sub(limits.max_windows);
    let mut candidates: Vec<&ContextState<'_>> = contexts
        .iter()
        .filter(|state| state.releasable() && !expired(state) && state.idle >= limits.grace)
        .collect();
    candidates.sort_by(|a, b| b.idle.cmp(&a.idle).then_with(|| a.key.cmp(b.key)));
    release.extend(
        candidates
            .into_iter()
            .take(excess)
            .map(|state| state.key.to_owned()),
    );
    release
}

#[cfg(test)]
#[path = "context_reaper_tests.rs"]
mod tests;
