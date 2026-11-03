//! Remembers which streams were found for the title being played, so the player can switch.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use app_services::PlaybackItem;
use core_model::{PlayableLink, SubtitleTrack};

/// What the check of one stream found out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceHealth {
    /// Not asked yet, or the answer is still on its way.
    Checking,
    /// The host answered like something a player can open.
    Working,
    /// The host did not answer properly; the stream is not offered.
    Broken,
}

/// The streams offered for the item that is playing now.
#[derive(Debug, Clone)]
pub struct ActiveSources {
    /// What is being watched; carried into saved progress when the source changes.
    pub item: PlaybackItem,
    /// Playable streams, best first.
    pub links: Vec<PlayableLink>,
    /// What the check found for each stream, in the same order as `links`.
    pub health: Vec<SourceHealth>,
    /// File size in bytes of each stream, when the check learned it.
    pub sizes: Vec<Option<u64>>,
    /// Languages of the subtitles each stream carries itself, as the check found them.
    pub stream_subtitles: Vec<Vec<String>>,
    /// External subtitles the provider offered.
    pub subtitles: Vec<SubtitleTrack>,
    /// Index into `links` of the stream that is playing.
    pub selected: usize,
    /// Last position seen while the stream was playing; a failed switch must not lose it.
    pub position_ms: i64,
    /// Identifies this set of streams so late check results for an older title are ignored.
    pub generation: u64,
    /// How many of `subtitles` the player has been given.
    pub subtitles_sent: usize,
    /// Whether a stream has started playing, so subtitles found now go straight to the player.
    pub playing: bool,
}

impl ActiveSources {
    /// Streams of a newly opened item, none of them checked yet and the first one selected.
    pub fn new(
        item: PlaybackItem,
        links: Vec<PlayableLink>,
        subtitles: Vec<SubtitleTrack>,
    ) -> Self {
        Self {
            health: vec![SourceHealth::Checking; links.len()],
            sizes: vec![None; links.len()],
            stream_subtitles: vec![Vec::new(); links.len()],
            item,
            links,
            subtitles,
            selected: 0,
            position_ms: 0,
            generation: 0,
            subtitles_sent: 0,
            playing: false,
        }
    }
}

/// Holder of the current [`ActiveSources`].
#[derive(Default)]
pub struct SourceSession {
    active: Mutex<Option<ActiveSources>>,
    generations: AtomicU64,
}

impl SourceSession {
    /// Remembers the streams of a newly started item and returns their generation number.
    pub fn set(&self, mut sources: ActiveSources) -> u64 {
        let generation = self.generations.fetch_add(1, Ordering::SeqCst) + 1;
        sources.generation = generation;
        *self.active.lock().unwrap_or_else(PoisonError::into_inner) = Some(sources);
        generation
    }

    /// Forgets the streams once playback is stopped.
    pub fn clear(&self) {
        *self.active.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// A copy of the current streams, if anything is playing.
    pub fn snapshot(&self) -> Option<ActiveSources> {
        self.active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether `generation` is still the title that is open (not replaced, not closed).
    pub fn is_current(&self, generation: u64) -> bool {
        self.active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|active| active.generation == generation)
    }

    /// Adds streams and subtitles the search just found. Returns the index the first new stream
    /// got, or `None` when `generation` is no longer the current one.
    pub fn append(
        &self,
        generation: u64,
        links: Vec<PlayableLink>,
        subtitles: Vec<SubtitleTrack>,
    ) -> Option<usize> {
        let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let active = guard.as_mut().filter(|a| a.generation == generation)?;
        let first = active.links.len();
        active
            .health
            .extend(std::iter::repeat_n(SourceHealth::Checking, links.len()));
        active.sizes.extend(std::iter::repeat_n(None, links.len()));
        active
            .stream_subtitles
            .extend(std::iter::repeat_n(Vec::new(), links.len()));
        active.links.extend(links);
        active.subtitles.extend(subtitles);
        Some(first)
    }

    /// A copy of the current streams for starting playback: the subtitles in it count as given
    /// to the player. Subtitles found from now on are held back until [`SourceSession::mark_selected`]
    /// says the new stream plays, so none is handed to the player while it is switching.
    pub fn start_snapshot(&self) -> Option<ActiveSources> {
        let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let active = guard.as_mut()?;
        active.subtitles_sent = active.subtitles.len();
        active.playing = false;
        Some(active.clone())
    }

    /// Subtitles found since the player was last given some, once a stream is playing.
    pub fn unsent_subtitles(&self, generation: u64) -> Vec<SubtitleTrack> {
        let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(active) = guard
            .as_mut()
            .filter(|a| a.generation == generation && a.playing)
        else {
            return Vec::new();
        };
        let fresh = active
            .subtitles
            .get(active.subtitles_sent..)
            .map(<[SubtitleTrack]>::to_vec)
            .unwrap_or_default();
        active.subtitles_sent = active.subtitles.len();
        fresh
    }

    /// Records what the check found for one stream. Returns `false` when `generation` is no
    /// longer the current one (another title was opened or playback stopped): stop checking.
    pub fn set_health(&self, generation: u64, index: usize, health: SourceHealth) -> bool {
        let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        match guard.as_mut() {
            Some(active) if active.generation == generation => {
                if let Some(slot) = active.health.get_mut(index) {
                    *slot = health;
                }
                true
            }
            _ => false,
        }
    }

    /// Records the file size the check learned for one stream; `false` when `generation` is no
    /// longer the current one.
    pub fn set_size(&self, generation: u64, index: usize, size_bytes: u64) -> bool {
        let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        match guard.as_mut() {
            Some(active) if active.generation == generation => {
                if let Some(slot) = active.sizes.get_mut(index) {
                    *slot = Some(size_bytes);
                }
                true
            }
            _ => false,
        }
    }

    /// Records the subtitle languages the check found inside one stream; `false` when
    /// `generation` is no longer the current one.
    pub fn set_stream_subtitles(
        &self,
        generation: u64,
        index: usize,
        languages: Vec<String>,
    ) -> bool {
        let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        match guard.as_mut() {
            Some(active) if active.generation == generation => {
                if let Some(slot) = active.stream_subtitles.get_mut(index) {
                    *slot = languages;
                }
                true
            }
            _ => false,
        }
    }

    /// Remembers how far playback got, ignoring the zero a failed or loading stream reports.
    pub fn remember_position(&self, position_ms: i64) {
        if position_ms <= 0 {
            return;
        }
        self.with_active(|active| active.position_ms = position_ms);
    }

    /// Records that the stream at `index` is now playing.
    pub fn mark_selected(&self, index: usize) {
        self.with_active(|active| {
            active.selected = index;
            active.playing = true;
        });
    }

    fn with_active(&self, change: impl FnOnce(&mut ActiveSources)) {
        if let Some(active) = self
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            change(active);
        }
    }
}

#[cfg(test)]
#[path = "source_session_tests.rs"]
mod tests;
