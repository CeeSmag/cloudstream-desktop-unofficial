//! Shrinking a kept verified window (plan step H4, `docs/MEMORY.md`).
//!
//! The check needs the real page, but a window kept only for the sidecar's browser fetches needs
//! nothing more than a document of the same origin to run `fetch` from. A passed window that has
//! been quiet for a while is moved to the site's `robots.txt`, a few bytes, so the heavy page's
//! memory is freed. A browser setting could not do that: the page is live.
//!
//! Pure: the caller says what the window is doing and how long it has been quiet.

use std::time::Duration;

use url::Url;

/// The small page: it exists on nearly every site and is plain text. A site without it answers a
/// 404 page, which is still a document of the right origin.
const SLIM_PATH: &str = "/robots.txt";

/// What decides whether to slim a window.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SlimState {
    /// The check passed.
    pub(crate) passed: bool,
    /// Already moved to the small page.
    pub(crate) slimmed: bool,
    /// A browser fetch is running in it.
    pub(crate) fetching: bool,
    /// Time since the window was last used.
    pub(crate) idle: Duration,
}

/// The small page of `target`'s site; `None` for an address that is not a web page.
pub(crate) fn slim_address(target: &Url) -> Option<Url> {
    matches!(target.scheme(), "http" | "https")
        .then(|| target.join(SLIM_PATH).ok())
        .flatten()
}

/// Whether to move the window to its small page now: passed, not already slim, no fetch running,
/// and quiet for at least `after` (so a fetch that is about to come meets the real page).
pub(crate) fn should_slim(state: &SlimState, after: Duration) -> bool {
    state.passed && !state.slimmed && !state.fetching && state.idle >= after
}

#[cfg(test)]
#[path = "context_slimming_tests.rs"]
mod tests;
