//! What was found for the titles looked up in this run, so an episode is asked about once.

use std::collections::{HashMap, VecDeque};

use crate::external_ids::ExternalIds;
use crate::skip_segment::{SkipQuery, SkipSegment, TitleKind};

/// How many titles' answers are kept; the oldest go first. An answer is a few dozen bytes.
pub(crate) const MAX_CACHED: usize = 64;

/// What identifies an answer: the video's length is left out, it does not change what exists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    ids: ExternalIds,
    kind: TitleKind,
    season: Option<u32>,
    episode: Option<u32>,
}

impl CacheKey {
    pub(crate) fn of(query: &SkipQuery) -> Self {
        Self {
            ids: query.ids.clone(),
            kind: query.kind,
            season: query.season,
            episode: query.episode,
        }
    }
}

/// A bounded memory of answers, oldest forgotten first.
#[derive(Default)]
pub(crate) struct SkipCache {
    entries: HashMap<CacheKey, Vec<SkipSegment>>,
    order: VecDeque<CacheKey>,
}

impl SkipCache {
    pub(crate) fn get(&self, key: &CacheKey) -> Option<Vec<SkipSegment>> {
        self.entries.get(key).cloned()
    }

    pub(crate) fn put(&mut self, key: CacheKey, segments: Vec<SkipSegment>) {
        if self.entries.insert(key.clone(), segments).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > MAX_CACHED {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}
