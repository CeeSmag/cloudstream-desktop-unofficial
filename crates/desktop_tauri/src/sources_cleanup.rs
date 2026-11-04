//! The folder of saved stream links: opening it at start and keeping it small for as long as the
//! app runs.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use title_cache::{CacheLimits, TitleCache};

/// How often old saved sources are looked for while the app stays open.
pub(crate) const CLEANUP_EVERY: Duration = Duration::from_secs(3600);

/// Opens the folder of saved sources under `data_dir`. Whatever was saved before `valid_from` (the
/// time of the last provider switch, 0 when there was none) stays unused.
pub(crate) fn open_title_cache(data_dir: &Path, valid_from: u64) -> Arc<TitleCache> {
    let cache = Arc::new(TitleCache::new(
        data_dir.join("sources"),
        CacheLimits::default(),
    ));
    cache.set_valid_from(valid_from);
    spawn_cleanup(&cache, CLEANUP_EVERY);
    cache
}

/// Removes expired, switched-away and surplus saved sources now and then every `every`, so what
/// is no longer used does not stay on disk. It stops by itself once nobody else holds the cache.
pub(crate) fn spawn_cleanup(cache: &Arc<TitleCache>, every: Duration) {
    let cache = Arc::downgrade(cache);
    let spawned = std::thread::Builder::new()
        .name("sources-cleanup".to_owned())
        .spawn(move || {
            while let Some(cache) = cache.upgrade() {
                cache.prune();
                drop(cache);
                std::thread::sleep(every);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot start the cleanup of saved sources");
    }
}

#[cfg(test)]
#[path = "sources_cleanup_tests.rs"]
mod tests;
