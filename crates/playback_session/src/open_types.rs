//! Types and hooks for a checked stream open.
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::LinkHealth;
use core_model::{PlayableLink, SubtitleTrack};
use thiserror::Error;
use title_cache::{CachedTitle, TitleCache};

/// Outcome of a checked open: streams ordered best-first with health, plus which to play.
#[derive(Debug, Clone, Default)]
pub struct CheckedOpen {
    /// Links in play order with their check result (`None` = still unchecked when we stopped).
    pub links: Vec<(PlayableLink, Option<LinkHealth>)>,
    /// Index into `links` of the stream to start, when one works.
    pub play_index: Option<usize>,
    /// Subtitles gathered from the provider search.
    pub subtitles: Vec<SubtitleTrack>,
}

/// Progress while searching and checking (for UI status lines).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenProgress {
    /// Streams found so far.
    pub found: usize,
    /// Streams already checked.
    pub checked: usize,
    /// Streams that answered as working.
    pub working: usize,
    /// Search from the provider has finished.
    pub search_done: bool,
}

/// Why a checked open failed before any playable stream was ready.
#[derive(Debug, Error)]
pub enum OpenError {
    /// Provider / gateway failure while starting or reading the search.
    #[error("{0}")]
    Search(String),
    /// Search finished with nothing the player can open.
    #[error("no playable streams found")]
    Empty,
}

/// Optional TitleCache + media key so a fresh/stale list can play before the live search finishes.
#[derive(Clone)]
pub struct OpenCache {
    /// On-disk stream cache.
    pub cache: Arc<TitleCache>,
    /// Title key used by the archived SourceManager (`provider\nmedia_key`).
    pub title_key: String,
}

impl OpenCache {
    /// Builds the usual title key used by the archived SourceManager.
    pub fn key(provider: &str, media_key: &str) -> String {
        format!("{provider}\n{media_key}")
    }
}

/// Callbacks and control flags for a live open.
pub struct OpenHooks<'a> {
    /// Counts change (found / checked / working).
    pub on_progress: &'a (dyn Fn(OpenProgress) + Sync),
    /// Full snapshot after each batch or check (for the sources list).
    pub on_update: &'a (dyn Fn(&CheckedOpen) + Sync),
    /// First time a working stream is ready to play (index into the current list).
    pub on_ready: &'a (dyn Fn(usize) + Sync),
    /// When true, stop waiting for more checks after a working stream exists (play best now).
    pub skip: &'a AtomicBool,
    /// When true, abandon the open entirely.
    pub cancel: &'a AtomicBool,
}

/// Stable title key helper for callers that already hold provider + media_key.
pub fn title_key(provider: &str, media_key: &str) -> String {
    OpenCache::key(provider, media_key)
}

/// Saves a completed open into the title cache (best-effort).
pub fn save_open(cache: &TitleCache, title_key: &str, opened: &CheckedOpen) {
    let links: Vec<title_cache::CachedLink> = opened
        .links
        .iter()
        .map(|(link, health)| title_cache::CachedLink {
            link: link.clone(),
            health: match health {
                Some(h) if h.is_working() => title_cache::LinkHealth::Working,
                Some(_) => title_cache::LinkHealth::Broken,
                None => title_cache::LinkHealth::Unchecked,
            },
        })
        .collect();
    let last_good = opened
        .play_index
        .and_then(|index| opened.links.get(index))
        .map(|(link, _)| link.url.clone());
    let title = CachedTitle {
        complete: true,
        links,
        last_good,
    };
    if let Err(error) = cache.save(title_key, &title) {
        tracing::warn!(%error, "could not save title sources");
    }
}
