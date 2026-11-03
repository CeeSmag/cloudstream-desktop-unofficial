use core_model::{Bookmark, WatchProgress};
use storage::{BookmarkStore, WatchProgressStore};

use crate::ServiceError;

/// How many "continue watching" entries the home screen shows.
const CONTINUE_WATCHING_LIMIT: u32 = 20;

/// The user's saved titles and watch history.
#[derive(Clone)]
pub struct LibraryService {
    bookmarks: BookmarkStore,
    progress: WatchProgressStore,
}

impl LibraryService {
    /// Creates the service over its stores.
    pub fn new(bookmarks: BookmarkStore, progress: WatchProgressStore) -> Self {
        Self {
            bookmarks,
            progress,
        }
    }

    /// Saves a title to the library.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn add_bookmark(&self, bookmark: &Bookmark) -> Result<(), ServiceError> {
        Ok(self.bookmarks.add(bookmark)?)
    }

    /// Removes a title from the library.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, url = ?url), err)]
    pub fn remove_bookmark(&self, provider: &str, url: &str) -> Result<(), ServiceError> {
        Ok(self.bookmarks.remove(provider, url)?)
    }

    /// Whether a title is in the library.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, url = ?url), err)]
    pub fn is_bookmarked(&self, provider: &str, url: &str) -> Result<bool, ServiceError> {
        Ok(self.bookmarks.contains(provider, url)?)
    }

    /// All saved titles, newest first.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn bookmarks(&self) -> Result<Vec<Bookmark>, ServiceError> {
        Ok(self.bookmarks.list()?)
    }

    /// Items to offer under "continue watching": started, not finished, newest first.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn continue_watching(&self) -> Result<Vec<WatchProgress>, ServiceError> {
        let recent = self.progress.recent(CONTINUE_WATCHING_LIMIT * 2)?;
        Ok(recent
            .into_iter()
            .filter(|entry| entry.position_ms > 0 && entry.fraction_watched() < 0.95)
            .take(CONTINUE_WATCHING_LIMIT as usize)
            .collect())
    }

    /// Takes one item out of "continue watching" by forgetting its progress. Returns whether it
    /// was there.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider), err)]
    pub fn remove_progress(&self, provider: &str, media_key: &str) -> Result<bool, ServiceError> {
        Ok(self.progress.remove(provider, media_key)?)
    }

    /// Empties "continue watching": forgets the progress of every item it can show, however many
    /// there are, and returns them. Items it never shows (not started, finished) are left alone.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn clear_continue_watching(&self) -> Result<Vec<WatchProgress>, ServiceError> {
        let mut removed = Vec::new();
        loop {
            let shown = self.continue_watching()?;
            if shown.is_empty() {
                return Ok(removed);
            }
            for entry in shown {
                self.progress.remove(&entry.provider, &entry.media_key)?;
                removed.push(entry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::Database;

    use super::*;

    fn service() -> (LibraryService, WatchProgressStore) {
        let database = Arc::new(Database::open_in_memory().expect("memory db"));
        let progress = WatchProgressStore::new(Arc::clone(&database));
        (
            LibraryService::new(BookmarkStore::new(database), progress.clone()),
            progress,
        )
    }

    fn entry(key: &str, position_ms: i64, duration_ms: i64, at: i64) -> WatchProgress {
        WatchProgress {
            provider: "P".into(),
            media_key: key.into(),
            title: key.into(),
            position_ms,
            duration_ms,
            updated_at_ms: at,
            ..WatchProgress::default()
        }
    }

    #[test]
    fn continue_watching_skips_unstarted_and_finished_items() {
        let (service, progress) = service();
        progress.save(&entry("unstarted", 0, 100, 1)).expect("save");
        progress.save(&entry("finished", 99, 100, 2)).expect("save");
        progress.save(&entry("midway", 40, 100, 3)).expect("save");
        let keys: Vec<_> = service
            .continue_watching()
            .expect("list")
            .into_iter()
            .map(|e| e.media_key)
            .collect();
        assert_eq!(keys, ["midway"]);
    }

    #[test]
    fn removing_one_item_leaves_the_others_in_continue_watching() {
        let (library, store) = service();
        for key in ["a", "b"] {
            store.save(&entry(key, 10, 100, 1)).expect("save");
        }
        assert!(library.remove_progress("P", "a").expect("removed"));
        assert!(!library.remove_progress("P", "a").expect("already gone"));
        let keys: Vec<_> = library
            .continue_watching()
            .expect("list")
            .into_iter()
            .map(|e| e.media_key)
            .collect();
        assert_eq!(keys, ["b"]);
    }

    #[test]
    fn clearing_continue_watching_removes_every_shown_item_even_past_the_list_limit() {
        let (library, store) = service();
        for n in 0..45 {
            store
                .save(&entry(&format!("k{n}"), 10, 100, i64::from(n)))
                .expect("save");
        }
        store.save(&entry("unstarted", 0, 100, 99)).expect("save");
        let removed = library.clear_continue_watching().expect("cleared");
        assert_eq!(removed.len(), 45);
        assert!(library.continue_watching().expect("list").is_empty());
        assert!(
            store.get("P", "unstarted").expect("get").is_some(),
            "never shown, never touched"
        );
    }

    #[test]
    fn bookmark_round_trip() {
        let (service, _) = service();
        let bookmark = Bookmark {
            provider: "P".into(),
            url: "u".into(),
            name: "n".into(),
            ..Bookmark::default()
        };
        service.add_bookmark(&bookmark).expect("add");
        assert!(service.is_bookmarked("P", "u").expect("check"));
        service.remove_bookmark("P", "u").expect("remove");
        assert!(service.bookmarks().expect("list").is_empty());
    }
}
