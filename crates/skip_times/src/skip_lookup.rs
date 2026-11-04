//! Asking the databases in turn until one knows.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::clean_segments::clean;
use crate::skip_cache::{CacheKey, SkipCache};
use crate::skip_fetcher::{SkipError, SkipFetcher};
use crate::skip_segment::{SkipQuery, SkipSegment, TitleKind};
use crate::{ani_skip, intro_db, the_intro_db};

type Source = fn(&dyn SkipFetcher, &SkipQuery) -> Result<Vec<SkipSegment>, SkipError>;

/// The reference app's order: AniSkip first for anime, then TheIntroDB, then IntroDB.
static ANIME: [(&str, Source); 2] = [
    ("AniSkip", ani_skip::segments),
    ("TheIntroDB", the_intro_db::segments),
];
static SERIES: [(&str, Source); 2] = [
    ("TheIntroDB", the_intro_db::segments),
    ("IntroDB", intro_db::segments),
];
static MOVIE: [(&str, Source); 1] = [("TheIntroDB", the_intro_db::segments)];

fn sources(kind: TitleKind) -> &'static [(&'static str, Source)] {
    match kind {
        TitleKind::Anime => &ANIME,
        TitleKind::Series => &SERIES,
        TitleKind::Movie => &MOVIE,
    }
}

/// Finds skip times for an episode or film, remembering the answers of this run.
pub struct SkipLookup {
    fetcher: Arc<dyn SkipFetcher>,
    cache: Mutex<SkipCache>,
}

impl SkipLookup {
    /// A lookup that asks through `fetcher`.
    pub fn new(fetcher: Arc<dyn SkipFetcher>) -> Self {
        Self {
            fetcher,
            cache: Mutex::new(SkipCache::default()),
        }
    }

    fn cache(&self) -> MutexGuard<'_, SkipCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The segments of the query, best database first. Empty when nobody knows (remembered, so it
    /// is not asked again) or when the title has no usable id. An error means the databases could
    /// not be asked; nothing is remembered then, so a later call tries again.
    #[tracing::instrument(level = "debug", skip_all, fields(kind = ?query.kind, season = ?query.season, episode = ?query.episode), err)]
    pub fn find(&self, query: &SkipQuery) -> Result<Vec<SkipSegment>, SkipError> {
        if query.ids.is_empty() {
            return Ok(Vec::new());
        }
        let key = CacheKey::of(query);
        if let Some(known) = self.cache().get(&key) {
            return Ok(known);
        }
        let found = self.ask(query)?;
        self.cache().put(key, found.clone());
        Ok(found)
    }

    fn ask(&self, query: &SkipQuery) -> Result<Vec<SkipSegment>, SkipError> {
        let mut failure = None;
        for (name, source) in sources(query.kind) {
            match source(self.fetcher.as_ref(), query) {
                Ok(found) => {
                    let found = clean(found, query.duration_ms);
                    if !found.is_empty() {
                        tracing::info!(source = name, segments = found.len(), "skip times found");
                        return Ok(found);
                    }
                }
                Err(error) => {
                    tracing::warn!(source = name, %error, "skip times database not answering");
                    failure = Some(error);
                }
            }
        }
        failure.map_or(Ok(Vec::new()), Err)
    }

    /// How many answers are remembered.
    pub fn remembered(&self) -> usize {
        self.cache().len()
    }
}

#[cfg(test)]
#[path = "skip_lookup_race_tests.rs"]
mod race_tests;
#[cfg(test)]
#[path = "skip_lookup_tests.rs"]
mod tests;
