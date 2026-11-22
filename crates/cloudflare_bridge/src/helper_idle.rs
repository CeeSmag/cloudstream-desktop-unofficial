//! When to close the verification helper (plan step 1f, `docs/MEMORY.md`).
//!
//! The helper is a browser engine (WebKitGTK / WebView2) with a hidden window per verified site:
//! 100-300 MB. It starts when a site needs a check and used to stay until the app closed. A quiet
//! helper is now closed after a limit the viewer chooses; the next check starts it again.

use std::path::Path;
use std::time::{Duration, SystemTime};

/// What the decision depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdleInputs {
    /// Close after this much quiet; `None` = never.
    pub(crate) ttl: Option<Duration>,
    /// Time since the app last sent the helper a command or heard a status from it.
    pub(crate) since_last_command: Duration,
    /// Checks still waiting for the viewer or running.
    pub(crate) pending_requests: usize,
    /// Age of the newest browser-fetch file in the exchange folder (the helper answers those
    /// itself, so the app sees no command for them); `None` when there is none.
    pub(crate) since_last_fetch_file: Option<Duration>,
}

/// Whether the helper has been quiet long enough to close.
pub(crate) fn should_stop(inputs: &IdleInputs) -> bool {
    let Some(ttl) = inputs.ttl else {
        return false;
    };
    inputs.pending_requests == 0
        && inputs.since_last_command >= ttl
        && inputs.since_last_fetch_file.is_none_or(|age| age >= ttl)
}

/// Age of the newest `fetch-*` / `fetchresult-*` file in `dir`, if any.
pub(crate) fn newest_fetch_file_age(dir: &Path) -> Option<Duration> {
    let now = SystemTime::now();
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("fetch"))
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .filter_map(|modified| now.duration_since(modified).ok())
        .min()
}

#[cfg(test)]
#[path = "helper_idle_tests.rs"]
mod tests;
