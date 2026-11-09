//! The folder of saved stream links: opening it at start and keeping it small for as long as the
//! app runs.

use std::path::Path;
use std::sync::Arc;

use title_cache::{CacheLimits, TitleCache};

/// Opens the folder of saved sources under `data_dir`. Whatever was saved before `valid_from` (the
/// time of the last provider switch, 0 when there was none) stays unused. Old entries are removed by
/// the cleanup queue (`cleanup_start`), not by a thread of their own.
pub(crate) fn open_title_cache(data_dir: &Path, valid_from: u64) -> Arc<TitleCache> {
    let cache = Arc::new(TitleCache::new(
        data_dir.join("sources"),
        CacheLimits::default(),
    ));
    cache.set_valid_from(valid_from);
    cache
}

#[cfg(test)]
#[path = "sources_cleanup_tests.rs"]
mod tests;
