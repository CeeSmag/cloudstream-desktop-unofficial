//! Downloads the subtitle files a provider offers, cleans them up (unpacks, decodes, detects the
//! kind) and keeps them on disk, so the player only ever opens small local files: they show up at
//! once, survive a change of source, and are still there when the provider's service is slow.
#![forbid(unsafe_code)]

mod hls_subtitles;
mod host_circuit;
mod normalize_subtitle;
mod subtitle_cache;
mod subtitle_cache_keep;
mod subtitle_fetcher;
mod subtitle_loader;
mod subtitle_prefetch;

pub use host_circuit::{host_of, HostCircuit, HOST_FAIL_THRESHOLD};
pub use normalize_subtitle::{normalize_subtitle, NormalizeError, NormalizedSubtitle};
pub use subtitle_cache::{CacheError, ReadySubtitle, SubtitleCache};
pub use subtitle_fetcher::{FetchError, SubtitleFetcher, UreqSubtitleFetcher};
pub use subtitle_loader::{LoadError, LoadOptions, LoadedSubtitle, SubtitleLoader};
pub use subtitle_prefetch::{
    FailureCallback, KeepGoing, LanguageRank, PrefetchCallbacks, PrefetchConfig, PrefetchSummary,
    ReadyCallback, SubtitlePrefetch, SKIP_DOWNLOAD_RANK,
};
