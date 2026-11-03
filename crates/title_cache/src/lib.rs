//! Remembers the stream links found for a title on disk, so opening it again is fast. The cache
//! never decides anything: a fresh search still runs, and any problem with the files simply means
//! "no cache". See the README.
#![forbid(unsafe_code)]

mod cache_limits;
mod cached_title;
mod redact_url;
mod stored_file;
mod title_cache;
mod title_cache_error;
mod title_prune;

#[cfg(test)]
mod test_folder;
#[cfg(test)]
mod title_cache_race_tests;

pub use cache_limits::CacheLimits;
pub use cached_title::{CachedLink, CachedTitle, Freshness, LinkHealth};
pub use redact_url::redact_url;
pub use stored_file::Miss;
pub use title_cache::{CacheHit, Clock, Lookup, SaveReport, TitleCache};
pub use title_cache_error::TitleCacheError;
pub use title_prune::PruneReport;
