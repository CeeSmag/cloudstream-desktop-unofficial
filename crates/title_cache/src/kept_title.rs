//! Only the video opened last keeps its saved sources.
//!
//! Opening a video removes what was saved for every other title, so the previous video searches
//! again from scratch when it is opened again. A save belongs to the opening that asked for it:
//! once a newer opening (of another title, or the same one opened again or refreshed) has taken
//! over, a late save or removal of the older one is refused. The check and the write happen under
//! the cache's one write lock, so a slow check of the previous video cannot bring its file back.

use std::path::Path;

use crate::cached_title::CachedTitle;
use crate::title_cache::{SaveReport, TitleCache, FILE_EXTENSION};
use crate::title_cache_error::TitleCacheError;

/// The one title whose saved sources are kept, handed out by [`TitleCache::keep_only`]. Only the
/// latest one handed out may save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptTitle {
    key: String,
    /// Which call of [`TitleCache::keep_only`] gave it; a later call makes it stale.
    pub(crate) ticket: u64,
}

/// What [`TitleCache::save_kept`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeptSave {
    /// The file was written.
    Saved(SaveReport),
    /// Another video was opened since (or this one again): nothing was written.
    Replaced,
}

impl TitleCache {
    /// Makes `title_key` the only title with saved sources: every other title's file is removed,
    /// its own is kept. Saves of earlier [`KeptTitle`]s are refused from now on.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn keep_only(&self, title_key: &str) -> KeptTitle {
        let mut latest = self.write_lock();
        *latest += 1;
        let removed = remove_all_but(&self.folder, &self.path_of(title_key));
        if removed > 0 {
            tracing::info!(
                removed,
                "dropped the saved sources of earlier videos; only the open one keeps its own"
            );
        }
        KeptTitle {
            key: title_key.to_owned(),
            ticket: *latest,
        }
    }

    /// Saves like [`TitleCache::save`], but only while `kept` is the latest title kept.
    pub fn save_kept(
        &self,
        kept: &KeptTitle,
        title: &CachedTitle,
    ) -> Result<KeptSave, TitleCacheError> {
        let latest = self.write_lock();
        if *latest != kept.ticket {
            tracing::debug!("another video was opened since; its sources are not saved");
            return Ok(KeptSave::Replaced);
        }
        self.save_locked(&kept.key, title).map(KeptSave::Saved)
    }
}

/// Removes every saved title in `folder` except the file at `keep`. Files being written (their
/// temporary names) are left to the cleanup. Returns how many were removed.
fn remove_all_but(folder: &Path, keep: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0; // nothing was ever saved
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.as_path() != keep)
        .filter(|path| path.extension().is_some_and(|ext| ext == FILE_EXTENSION))
        .filter(|path| match std::fs::remove_file(path) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "cannot remove the saved sources of an earlier video");
                false
            }
        })
        .count()
}

#[cfg(test)]
#[path = "kept_title_tests.rs"]
mod tests;
