//! Saved sources for tests: a cache in a folder of its own, removed afterwards, and a clock that
//! only moves when told to, so a test can make saved sources old.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use title_cache::{CacheLimits, TitleCache};

static FOLDERS: AtomicU32 = AtomicU32::new(0);

/// Seconds since 1970 the test clock starts at.
const START: u64 = 1_700_000_000;

/// A cache over its own folder with a clock of its own.
pub(crate) struct TestTitles {
    pub(crate) cache: Arc<TitleCache>,
    seconds: Arc<AtomicU64>,
    folder: PathBuf,
}

impl TestTitles {
    pub(crate) fn new() -> Self {
        let folder = std::env::temp_dir().join(format!(
            "cs_desktop_titles_{}_{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::SeqCst)
        ));
        let seconds = Arc::new(AtomicU64::new(START));
        let clock = Arc::clone(&seconds);
        let cache = Arc::new(TitleCache::with_clock(
            folder.clone(),
            CacheLimits::default(),
            Arc::new(move || UNIX_EPOCH + Duration::from_secs(clock.load(Ordering::SeqCst))),
        ));
        Self {
            cache,
            seconds,
            folder,
        }
    }

    /// Moves the clock on, making everything saved so far older.
    pub(crate) fn pass(&self, time: Duration) {
        self.seconds.fetch_add(time.as_secs(), Ordering::SeqCst);
    }
}

impl Drop for TestTitles {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.folder) {
            tracing::trace!(%error, "no folder of saved sources to remove");
        }
    }
}

/// A cache for tests that never use saved sources; its folder is not created.
pub(crate) fn unused_titles() -> Arc<TitleCache> {
    Arc::clone(&TestTitles::new().cache)
}
