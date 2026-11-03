//! Helpers shared by the tests: a temp folder, a clock to set by hand, and links to save.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use core_model::PlayableLink;

use crate::cache_limits::CacheLimits;
use crate::cached_title::{CachedLink, CachedTitle, LinkHealth};
use crate::title_cache::{Clock, TitleCache};

static FOLDERS: AtomicU32 = AtomicU32::new(0);

/// Seconds since 1970 the fake clock starts at.
pub(crate) const START: u64 = 1_700_000_000;

/// A folder of its own that is removed when the test ends.
pub(crate) struct Scratch(pub(crate) PathBuf);

impl Scratch {
    pub(crate) fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "cs_title_cache_{}_{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::SeqCst)
        )))
    }

    /// A cache here that tells time with `clock`.
    pub(crate) fn cache(&self, limits: CacheLimits, clock: &Time) -> TitleCache {
        TitleCache::with_clock(self.0.clone(), limits, clock.as_clock())
    }

    /// The names of the files in the folder, sorted.
    pub(crate) fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            // The folder does not exist when the test never saved anything.
            tracing::trace!(%error, "no scratch folder to remove");
        }
    }
}

/// A clock that only moves when told to.
#[derive(Clone)]
pub(crate) struct Time(Arc<AtomicU64>);

impl Time {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicU64::new(START)))
    }

    pub(crate) fn advance(&self, by: Duration) {
        self.0.fetch_add(by.as_secs(), Ordering::SeqCst);
    }

    pub(crate) fn set(&self, seconds: u64) {
        self.0.store(seconds, Ordering::SeqCst);
    }

    pub(crate) fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    pub(crate) fn as_clock(&self) -> Clock {
        let seconds = Arc::clone(&self.0);
        Arc::new(move || UNIX_EPOCH + Duration::from_secs(seconds.load(Ordering::SeqCst)))
    }
}

/// The real clock, for tests that look at file times.
pub(crate) fn real_now() -> SystemTime {
    SystemTime::now()
}

pub(crate) fn link(url: &str, quality: i32) -> PlayableLink {
    PlayableLink {
        source: "test".to_owned(),
        name: format!("source {quality}"),
        url: url.to_owned(),
        quality,
        ..PlayableLink::default()
    }
}

pub(crate) fn cached(url: &str, quality: i32, health: LinkHealth) -> CachedLink {
    CachedLink {
        link: link(url, quality),
        health,
    }
}

pub(crate) fn title(links: Vec<CachedLink>) -> CachedTitle {
    CachedTitle {
        complete: true,
        links,
        last_good: None,
    }
}

/// A title with `count` unchecked links named `{prefix}-{n}` whose quality rises with `n`.
pub(crate) fn many(prefix: &str, count: usize) -> CachedTitle {
    title(
        (0..count)
            .map(|n| {
                cached(
                    &format!("https://h.test/{prefix}-{n}"),
                    n as i32,
                    LinkHealth::Unchecked,
                )
            })
            .collect(),
    )
}
