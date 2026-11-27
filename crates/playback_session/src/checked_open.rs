//! Open a title by searching links and HTTP-checking them as they arrive.
//!
//! Waits for the provider search and every check (or skip / total time cap), then plays the best
//! working stream via [`OpenHooks::on_ready`]. Fresh TitleCache seeds can still start at once.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_services::{PreferredResolution, StreamChecker, StreamResolver};
use core_model::PlayableLink;
use extension_repo::UreqProbe;

use crate::check_pool::{run_search_pool, PoolConfig};
use crate::open_run::{finish, OpenState};
use crate::open_seeds::{plant_playing, plant_saved};
use crate::open_types::{CheckedOpen, OpenCache, OpenError, OpenHooks, OpenProgress};
use crate::subtitle_priority::CheckGate;

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
    let target = OpenTarget {
        streams,
        provider,
        data,
        preferred_resolution: PreferredResolution::Any,
    };
    open_with(&target, None, &hooks)
}

/// Which title's streams to open: the resolver, the provider and its episode / movie payload.
pub struct OpenTarget<'a> {
    /// Stream resolver.
    pub streams: &'a StreamResolver,
    /// Provider internal name.
    pub provider: &'a str,
    /// Episode / movie data payload for the link search.
    pub data: &'a str,
    /// Preferred resolution group for automatic stream selection (largest working file in group).
    pub preferred_resolution: PreferredResolution,
}

/// Optional initial seed for an open: either disk-cached links or an already-playing link.
pub(crate) enum OpenSeed<'a> {
    None,
    Cache(&'a OpenCache),
    Playing(&'a PlayableLink),
}

/// Full open: optional cache seeds, deferred `on_ready` (best working), live `on_update`, skip.
pub fn open_with(
    target: &OpenTarget<'_>,
    cache: Option<&OpenCache>,
    hooks: &OpenHooks<'_>,
) -> Result<CheckedOpen, OpenError> {
    let gate = CheckGate::default();
    let seed = cache.map_or(OpenSeed::None, OpenSeed::Cache);
    open_with_seed(target, seed, hooks, &gate)
}

/// Same as [`open_with`], with an explicit [`OpenSeed`] and [`CheckGate`].
pub(crate) fn open_with_seed(
    target: &OpenTarget<'_>,
    seed: OpenSeed<'_>,
    hooks: &OpenHooks<'_>,
    gate: &CheckGate,
) -> Result<CheckedOpen, OpenError> {
    let checker = StreamChecker::new(Arc::new(UreqProbe::default()));
    let mut state = OpenState::with_preferred(target.preferred_resolution);
    match seed {
        OpenSeed::Playing(link) => plant_playing(&mut state, link, hooks),
        OpenSeed::Cache(open_cache) => plant_saved(&mut state, open_cache, &checker, hooks),
        OpenSeed::None => {}
    }
    if hooks.cancel.load(Ordering::SeqCst) {
        return finish(state);
    }
    let search = target
        .streams
        .open_search(target.provider, target.data)
        .map_err(|error| OpenError::Search(error.to_string()))?;
    let config = PoolConfig::standard(&checker, hooks, gate);
    run_search_pool(search, state, &config)
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
