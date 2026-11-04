//! The saved stream links of titles, one small file each in a folder of the app's data.
//!
//! Every problem here means "treat it as not saved": a file that cannot be read or understood is
//! a miss (and removed when it can never be useful), and a failed save is reported to the caller
//! who carries on without the cache. Reading takes no lock, since a file is only ever replaced
//! whole by a rename; writing takes one lock so two saves never interleave.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use core_model::stable_key;

use crate::cache_limits::{CacheLimits, Verdict};
use crate::cached_title::{CachedTitle, Freshness, LinkHealth};
use crate::stored_file::{read_stored, write_stored, Miss, StoredFile, FORMAT_VERSION};
use crate::title_cache_error::TitleCacheError;

/// Tells the cache what time it is; tests set their own.
pub type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

const FILE_EXTENSION: &str = "json";

/// Saved links found for a title.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheHit {
    /// The content, links in playing order.
    pub title: CachedTitle,
    /// Whether to play at once or test first.
    pub freshness: Freshness,
    /// How long ago it was saved; unknown when the clock went backwards.
    pub age: Option<Duration>,
}

/// The result of asking for a title.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// Something usable was saved.
    Hit(CacheHit),
    /// Nothing usable; the reason is for logs and tests.
    Miss(Miss),
}

/// What a save wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveReport {
    /// Links in the file.
    pub links: usize,
    /// Size of the file.
    pub bytes: u64,
}

/// The folder of saved sources.
pub struct TitleCache {
    pub(crate) folder: PathBuf,
    pub(crate) limits: CacheLimits,
    clock: Clock,
    writing: Mutex<()>,
    pub(crate) counter: AtomicU64,
    /// Saves made before this time (seconds since 1970) are not used; 0 lets every save count.
    valid_from: AtomicU64,
}

impl TitleCache {
    /// A cache in `folder` that follows `limits` and the system clock.
    pub fn new(folder: PathBuf, limits: CacheLimits) -> Self {
        Self::with_clock(folder, limits, Arc::new(SystemTime::now))
    }

    /// Like [`TitleCache::new`], with a clock of your own.
    pub fn with_clock(folder: PathBuf, limits: CacheLimits, clock: Clock) -> Self {
        Self {
            folder,
            limits,
            clock,
            writing: Mutex::new(()),
            counter: AtomicU64::new(0),
            valid_from: AtomicU64::new(0),
        }
    }

    /// Takes the one write lock; a thread that panicked while holding it left nothing half-done,
    /// since files are replaced whole.
    pub(crate) fn write_lock(&self) -> MutexGuard<'_, ()> {
        self.writing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn now_secs(&self) -> u64 {
        (self.clock)()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs())
    }

    /// From now on, only what is saved from now on is used: everything saved before is treated as
    /// not saved and is removed by the next cleanup. Called when the viewer switches provider.
    /// Returns the time to remember (see [`TitleCache::set_valid_from`]).
    pub fn invalidate_now(&self) -> u64 {
        let now = self.now_secs();
        self.set_valid_from(now);
        now
    }

    /// Makes saves made before `seconds` (since 1970) unusable; the app calls it at start with the
    /// time of the last provider switch. A time earlier than the current one changes nothing.
    pub fn set_valid_from(&self, seconds: u64) {
        let before = self.valid_from.fetch_max(seconds, Ordering::SeqCst);
        if seconds > before {
            tracing::info!(
                valid_from = seconds,
                "saved sources from before this time are dropped"
            );
        }
    }

    /// What the age of a save means: the cache rules, after the provider switch.
    pub(crate) fn verdict(&self, saved_at: u64) -> Verdict {
        if saved_at < self.valid_from.load(Ordering::SeqCst) {
            return Verdict::Invalidated;
        }
        self.limits.judge(saved_at, self.now_secs())
    }

    /// Where the file of `title_key` is.
    pub(crate) fn path_of(&self, title_key: &str) -> PathBuf {
        self.folder
            .join(format!("{}.{FILE_EXTENSION}", stable_key(title_key)))
    }

    /// The saved sources of the title, if there are usable ones.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn load(&self, title_key: &str) -> Lookup {
        let path = self.path_of(title_key);
        let stored = match read_stored(&path, self.limits.max_file_bytes) {
            Ok(stored) => stored,
            Err(miss) => return self.missed(&path, miss),
        };
        match self.verdict(stored.saved_at) {
            Verdict::Expired => self.missed(&path, Miss::Expired),
            Verdict::Invalidated => self.missed(&path, Miss::Invalidated),
            Verdict::Usable(freshness, age) => {
                let title = stored.title.normalized(self.limits.max_links);
                tracing::debug!(
                    links = title.links.len(),
                    ?freshness,
                    ?age,
                    "saved sources found"
                );
                Lookup::Hit(CacheHit {
                    title,
                    freshness,
                    age,
                })
            }
        }
    }

    fn missed(&self, path: &Path, miss: Miss) -> Lookup {
        if miss.is_discardable() {
            tracing::warn!(?miss, "saved sources unusable; removing them");
            self.discard(path);
        } else {
            tracing::debug!(?miss, "no saved sources");
        }
        Lookup::Miss(miss)
    }

    /// Removes a file that was unusable, unless a save replaced it with a good one meanwhile.
    fn discard(&self, path: &Path) {
        let _lock = self.write_lock();
        let still_unusable = match read_stored(path, self.limits.max_file_bytes) {
            Ok(stored) => !matches!(self.verdict(stored.saved_at), Verdict::Usable(..)),
            Err(miss) => miss.is_discardable(),
        };
        if still_unusable {
            if let Err(error) = std::fs::remove_file(path) {
                tracing::warn!(%error, "cannot remove unusable saved sources");
            }
        }
    }

    /// Saves the title's links (put in playing order, deduplicated, cut to the limit), replacing
    /// what was saved. The save time is now.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn save(
        &self,
        title_key: &str,
        title: &CachedTitle,
    ) -> Result<SaveReport, TitleCacheError> {
        let _lock = self.write_lock();
        let stored = StoredFile {
            version: FORMAT_VERSION,
            saved_at: self.now_secs(),
            title: title.normalized(self.limits.max_links),
        };
        let report = self.write(title_key, &stored)?;
        tracing::debug!(
            links = report.links,
            bytes = report.bytes,
            complete = title.complete,
            "sources saved"
        );
        self.prune_locked();
        Ok(report)
    }

    fn write(&self, title_key: &str, stored: &StoredFile) -> Result<SaveReport, TitleCacheError> {
        std::fs::create_dir_all(&self.folder)
            .map_err(|error| TitleCacheError::disk("create", &self.folder, error))?;
        let path = self.path_of(title_key);
        let bytes = write_stored(&path, stored, &self.counter, self.limits.max_file_bytes)?;
        Ok(SaveReport {
            links: stored.title.links.len(),
            bytes,
        })
    }

    /// Changes the saved title without renewing its save time, so notes about one link do not make
    /// old links look fresh. Returns whether there was a usable file to change.
    pub fn update(
        &self,
        title_key: &str,
        change: impl FnOnce(&mut CachedTitle),
    ) -> Result<bool, TitleCacheError> {
        let _lock = self.write_lock();
        let path = self.path_of(title_key);
        let Ok(mut stored) = read_stored(&path, self.limits.max_file_bytes) else {
            return Ok(false);
        };
        if !matches!(self.verdict(stored.saved_at), Verdict::Usable(..)) {
            return Ok(false);
        }
        change(&mut stored.title);
        self.write(title_key, &stored)?;
        Ok(true)
    }

    /// Notes that the link at `url` turned out broken.
    pub fn mark_broken(&self, title_key: &str, url: &str) -> Result<bool, TitleCacheError> {
        self.update(title_key, |title| title.set_health(url, LinkHealth::Broken))
    }

    /// Notes that the link at `url` played well.
    pub fn mark_played_well(&self, title_key: &str, url: &str) -> Result<bool, TitleCacheError> {
        self.update(title_key, |title| title.set_played_well(url))
    }

    /// Removes the saved sources of the title (it was watched to the end). Returns whether there
    /// was a file.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn forget(&self, title_key: &str) -> Result<bool, TitleCacheError> {
        let _lock = self.write_lock();
        let path = self.path_of(title_key);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(TitleCacheError::disk("remove", &path, error)),
        }
    }
}

#[cfg(test)]
#[path = "title_cache_switch_tests.rs"]
mod switch_tests;
#[cfg(test)]
#[path = "title_cache_tests.rs"]
mod tests;
