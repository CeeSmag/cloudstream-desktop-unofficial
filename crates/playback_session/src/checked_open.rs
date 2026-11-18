//! Open a title by searching links and HTTP-checking them as they arrive.
//!
//! Plays as soon as the first working stream is found (via [`OpenHooks::on_ready`]) while search
//! and checks continue until the provider finishes, the total time cap, or the caller skips /
//! cancels. Unlike the old early-stop (two extras after the first), every candidate is checked.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_services::{StreamChecker, StreamResolver};
use extension_repo::UreqProbe;

use crate::open_run::{finish, plant_saved, run_search, OpenState};
use crate::open_types::{CheckedOpen, OpenCache, OpenError, OpenHooks, OpenProgress};

/// Searches for streams of `provider`/`data`, checks them, and picks the best working one.
///
/// Prefer [`open_with`] when the UI needs early play + live source updates.
pub fn check_and_pick(
    streams: &StreamResolver,
    provider: &str,
    data: &str,
    on_progress: &(dyn Fn(OpenProgress) + Sync),
) -> Result<CheckedOpen, OpenError> {
    let skip = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let hooks = OpenHooks {
        on_progress,
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    open_with(streams, provider, data, None, &hooks)
}

/// Full open: optional cache seeds, early `on_ready`, live `on_update`, skip / cancel.
pub fn open_with(
    streams: &StreamResolver,
    provider: &str,
    data: &str,
    cache: Option<&OpenCache>,
    hooks: &OpenHooks<'_>,
) -> Result<CheckedOpen, OpenError> {
    let checker = StreamChecker::new(Arc::new(UreqProbe::default()));
    let mut state = OpenState::new();
    if let Some(open_cache) = cache {
        plant_saved(&mut state, open_cache, &checker, hooks);
    }
    if hooks.cancel.load(Ordering::SeqCst) {
        return finish(state);
    }
    let search = streams
        .open_search(provider, data)
        .map_err(|error| OpenError::Search(error.to_string()))?;
    run_search(search, state, &checker, hooks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_types::title_key;

    #[test]
    fn open_progress_defaults_are_zero() {
        let progress = OpenProgress {
            found: 0,
            checked: 0,
            working: 0,
            search_done: false,
        };
        assert_eq!(progress.found, 0);
    }

    #[test]
    fn title_key_is_stable() {
        assert_eq!(title_key("P", "m"), title_key("P", "m"));
        assert_ne!(title_key("P", "a"), title_key("P", "b"));
    }
}
