use core_model::{Bookmark, WatchProgress};
use storage::{BookmarkStore, ResumeStore, WatchProgressStore};

use crate::ServiceError;

/// How many "continue watching" entries the home screen shows.
const CONTINUE_WATCHING_LIMIT: u32 = 20;

/// The user's saved titles and watch history.
#[derive(Clone)]
pub struct LibraryService {
    bookmarks: BookmarkStore,
    progress: WatchProgressStore,
    resume: ResumeStore,
}

impl LibraryService {
    /// Creates the service over its stores.
    pub fn new(
        bookmarks: BookmarkStore,
        progress: WatchProgressStore,
        resume: ResumeStore,
    ) -> Self {
        Self {
            bookmarks,
            progress,
            resume,
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

    /// Items to offer under "continue watching": one card per show, newest first.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn continue_watching(&self) -> Result<Vec<WatchProgress>, ServiceError> {
        let pointers = self.resume.recent(CONTINUE_WATCHING_LIMIT)?;
        let mut results = Vec::with_capacity(pointers.len());
        for pointer in pointers {
            let (pos, dur) = match self.progress.get(&pointer.provider, &pointer.media_key)? {
                Some(saved) => (saved.position_ms, saved.duration_ms),
                None => (0, 0),
            };
            results.push(WatchProgress {
                provider: pointer.provider,
                media_key: pointer.media_key,
                title: pointer.title,
                detail_url: pointer.detail_url,
                poster_url: pointer.poster_url,
                position_ms: pos,
                duration_ms: dur,
                updated_at_ms: pointer.updated_at_ms,
                season: pointer.season,
                episode: pointer.episode,
            });
        }
        Ok(results)
    }

    /// Reads the resume pointer for a specific show or movie.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
    pub fn resume_pointer(
        &self,
        provider: &str,
        detail_url: &str,
    ) -> Result<Option<WatchProgress>, ServiceError> {
        let pointer = match self.resume.get(provider, detail_url)? {
            Some(p) => p,
            None => return Ok(None),
        };
        let (pos, dur) = match self.progress.get(&pointer.provider, &pointer.media_key)? {
            Some(saved) => (saved.position_ms, saved.duration_ms),
            None => (0, 0),
        };
        Ok(Some(WatchProgress {
            provider: pointer.provider,
            media_key: pointer.media_key,
            title: pointer.title,
            detail_url: pointer.detail_url,
            poster_url: pointer.poster_url,
            position_ms: pos,
            duration_ms: dur,
            updated_at_ms: pointer.updated_at_ms,
            season: pointer.season,
            episode: pointer.episode,
        }))
    }

    /// The saved position of every episode of a show, for the marks on its title page.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
    pub fn show_progress(
        &self,
        provider: &str,
        detail_url: &str,
    ) -> Result<Vec<WatchProgress>, ServiceError> {
        Ok(self.progress.for_show(provider, detail_url)?)
    }

    /// Takes one item out of "continue watching" by forgetting its progress.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider), err)]
    pub fn remove_progress(&self, provider: &str, media_key: &str) -> Result<bool, ServiceError> {
        Ok(self.progress.remove(provider, media_key)?)
    }

    /// Removes a show from Continue Watching: deletes its resume pointer and all episode watch
    /// progress records, returning all episode media keys that were removed.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
    pub fn remove_continue_watching(
        &self,
        provider: &str,
        detail_url: &str,
    ) -> Result<Vec<String>, ServiceError> {
        let mut keys = self.progress.episodes_for_show(provider, detail_url)?;
        if let Some(pointer) = self.resume.get(provider, detail_url)? {
            if !keys.contains(&pointer.media_key) {
                keys.push(pointer.media_key);
            }
        }
        self.resume.remove(provider, detail_url)?;
        self.progress.remove_for_show(provider, detail_url)?;
        Ok(keys)
    }

    /// Flexible remover supporting either detail_url or fallback media_key.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider), err)]
    pub fn remove_continue_watching_entry(
        &self,
        provider: &str,
        detail_url: Option<&str>,
        media_key: Option<&str>,
    ) -> Result<Vec<String>, ServiceError> {
        let resolved_url = match detail_url.filter(|u| !u.is_empty()) {
            Some(u) => Some(u.to_owned()),
            None => match media_key {
                Some(key) => self
                    .progress
                    .get(provider, key)?
                    .map(|p| p.detail_url)
                    .filter(|u| !u.is_empty()),
                None => None,
            },
        };
        match resolved_url {
            Some(url) => self.remove_continue_watching(provider, &url),
            None => {
                if let Some(key) = media_key {
                    self.progress.remove(provider, key)?;
                    Ok(vec![key.to_owned()])
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    /// Empties Continue Watching: forgets all resume pointers and progress records, returning
    /// all (provider, media_key) pairs cleared.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn clear_continue_watching(&self) -> Result<Vec<(String, String)>, ServiceError> {
        let mut keys = self.progress.all_media_keys()?;
        let pointers = self.resume.all()?;
        for pointer in pointers {
            let pair = (pointer.provider, pointer.media_key);
            if !keys.contains(&pair) {
                keys.push(pair);
            }
        }
        self.resume.clear()?;
        self.progress.clear()?;
        Ok(keys)
    }
}

#[cfg(test)]
#[path = "library_service_tests.rs"]
mod tests;
