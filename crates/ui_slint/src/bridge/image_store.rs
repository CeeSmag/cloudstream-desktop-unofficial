//! One decoded picture per poster address, shared by every tile that shows it.
//!
//! Memory rule: the pictures held are those on screen (pinned) plus a few just released (kept, up
//! to the budget, so going back is instant). A pin is owned by a screen (`PosterKind`): the owner
//! lists the addresses it shows, and replacing that list is how a screen lets go. Only unpinned
//! pictures are ever evicted, least recently used first, so what the viewer sees never vanishes.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use slint::{Rgba8Pixel, SharedPixelBuffer};

use crate::ui_events::PosterKind;

type Pixels = SharedPixelBuffer<Rgba8Pixel>;

struct Entry {
    pixels: Pixels,
    bytes: usize,
    last_used: u64,
}

/// The pictures, their pins and the byte budget for those not pinned.
pub(crate) struct ImageStore {
    budget_bytes: usize,
    used_bytes: usize,
    clock: u64,
    entries: HashMap<String, Entry>,
    pins: HashMap<PosterKind, HashSet<String>>,
}

impl ImageStore {
    pub(crate) fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            used_bytes: 0,
            clock: 0,
            entries: HashMap::new(),
            pins: HashMap::new(),
        }
    }

    /// The picture for `url`, marked as just used.
    pub(crate) fn get(&mut self, url: &str) -> Option<Pixels> {
        self.clock += 1;
        let now = self.clock;
        self.entries.get_mut(url).map(|entry| {
            entry.last_used = now;
            entry.pixels.clone()
        })
    }

    /// Files `pixels` under `url` and returns the buffer every tile of that address shares: the one
    /// already stored when there is one, else `pixels`.
    pub(crate) fn insert(&mut self, url: &str, pixels: Pixels) -> Pixels {
        if let Some(stored) = self.get(url) {
            return stored;
        }
        let bytes = pixels.as_bytes().len();
        self.entries.insert(
            url.to_owned(),
            Entry {
                pixels: pixels.clone(),
                bytes,
                last_used: self.clock,
            },
        );
        self.used_bytes += bytes;
        self.evict_over_budget();
        pixels
    }

    /// Sets the addresses `owner` shows, replacing what it listed before; released pictures can be
    /// evicted from now on.
    pub(crate) fn set_pins(&mut self, owner: PosterKind, urls: impl IntoIterator<Item = String>) {
        self.pins.insert(owner, urls.into_iter().collect());
        self.evict_over_budget();
    }

    /// Whether any screen shows `url`.
    pub(crate) fn is_pinned(&self, url: &str) -> bool {
        self.pins.values().any(|urls| urls.contains(url))
    }

    /// Bytes of pixels held.
    #[cfg(test)]
    pub(crate) fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    /// Pictures held.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    fn evict_over_budget(&mut self) {
        while self.used_bytes > self.budget_bytes {
            let Some(victim) = self.least_recently_used_unpinned() else {
                tracing::trace!(
                    used_bytes = self.used_bytes,
                    budget_bytes = self.budget_bytes,
                    "pictures on screen exceed the budget; none can be evicted"
                );
                return;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.used_bytes = self.used_bytes.saturating_sub(entry.bytes);
            }
        }
    }

    fn least_recently_used_unpinned(&self) -> Option<String> {
        self.entries
            .iter()
            .filter(|(url, _)| !self.is_pinned(url))
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(url, _)| url.clone())
    }
}

/// The store, shared with the threads that finish downloads.
#[derive(Clone)]
pub(crate) struct SharedImageStore(Arc<Mutex<ImageStore>>);

impl SharedImageStore {
    pub(crate) fn new(budget_bytes: usize) -> Self {
        Self(Arc::new(Mutex::new(ImageStore::new(budget_bytes))))
    }

    /// Runs `action` with the store locked.
    pub(crate) fn with<R>(&self, action: impl FnOnce(&mut ImageStore) -> R) -> R {
        let mut store = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        action(&mut store)
    }
}

#[cfg(test)]
#[path = "image_store_tests.rs"]
mod tests;
