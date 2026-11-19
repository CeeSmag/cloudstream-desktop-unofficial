//! Decoded posters kept in memory, least recently used dropped first, within a byte budget.

use std::collections::HashMap;
use std::sync::Arc;

use crate::DecodedArtwork;

/// Posters kept in memory: an address, its pixels and when it was last used.
pub(crate) struct MemoryCache {
    budget_bytes: usize,
    used_bytes: usize,
    clock: u64,
    entries: HashMap<String, (Arc<DecodedArtwork>, u64)>,
}

impl MemoryCache {
    pub(crate) fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            used_bytes: 0,
            clock: 0,
            entries: HashMap::new(),
        }
    }

    /// The poster for `url`, marked as just used.
    pub(crate) fn get(&mut self, url: &str) -> Option<Arc<DecodedArtwork>> {
        self.clock += 1;
        let now = self.clock;
        self.entries.get_mut(url).map(|(artwork, used)| {
            *used = now;
            Arc::clone(artwork)
        })
    }

    /// Keeps `artwork` for `url`, dropping the least recently used posters to stay in budget.
    pub(crate) fn insert(&mut self, url: String, artwork: Arc<DecodedArtwork>) {
        self.clock += 1;
        let size = artwork.byte_size();
        if let Some((old, _)) = self.entries.insert(url, (artwork, self.clock)) {
            self.used_bytes = self.used_bytes.saturating_sub(old.byte_size());
        }
        self.used_bytes += size;
        while self.used_bytes > self.budget_bytes && self.entries.len() > 1 {
            self.drop_oldest();
        }
    }

    /// Drops the decoded poster for `url` if present.
    pub(crate) fn forget(&mut self, url: &str) -> bool {
        if let Some((artwork, _)) = self.entries.remove(url) {
            self.used_bytes = self.used_bytes.saturating_sub(artwork.byte_size());
            true
        } else {
            false
        }
    }

    fn drop_oldest(&mut self) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, (_, used))| *used)
            .map(|(url, _)| url.clone());
        if let Some((artwork, _)) = oldest.and_then(|url| self.entries.remove(&url)) {
            self.used_bytes = self.used_bytes.saturating_sub(artwork.byte_size());
        }
    }

    #[cfg(test)]
    pub(crate) fn used_bytes(&self) -> usize {
        self.used_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artwork(bytes: usize) -> Arc<DecodedArtwork> {
        Arc::new(DecodedArtwork {
            width: 1,
            height: 1,
            rgba: vec![0; bytes],
        })
    }

    #[test]
    fn insert_over_budget_drops_least_recently_used() {
        let mut cache = MemoryCache::new(250);
        cache.insert("a".into(), artwork(100));
        cache.insert("b".into(), artwork(100));
        assert!(cache.get("a").is_some(), "touch a so b is the oldest");
        cache.insert("c".into(), artwork(100));
        assert!(cache.get("b").is_none());
        assert!(cache.get("a").is_some() && cache.get("c").is_some());
        assert_eq!(cache.used_bytes(), 200);
    }

    #[test]
    fn insert_same_url_twice_counts_bytes_once() {
        let mut cache = MemoryCache::new(1000);
        cache.insert("a".into(), artwork(100));
        cache.insert("a".into(), artwork(300));
        assert_eq!(cache.used_bytes(), 300);
    }

    #[test]
    fn insert_single_poster_bigger_than_budget_is_still_kept() {
        let mut cache = MemoryCache::new(10);
        cache.insert("big".into(), artwork(100));
        assert!(cache.get("big").is_some());
    }

    #[test]
    fn forget_removes_bytes_and_entry() {
        let mut cache = MemoryCache::new(1000);
        cache.insert("a".into(), artwork(100));
        assert!(cache.forget("a"));
        assert!(cache.get("a").is_none());
        assert_eq!(cache.used_bytes(), 0);
        assert!(!cache.forget("a"));
    }
}
