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
    /// External subtitles the provider offered.
    pub subtitles: Vec<SubtitleTrack>,
    /// Index into `links` of the stream that is playing.
    pub selected: usize,
    /// Last position seen while the stream was playing; a failed switch must not lose it.
    pub position_ms: i64,
    /// Identifies this set of streams so late check results for an older title are ignored.
    pub generation: u64,
}

impl ActiveSources {
    /// Streams of a newly opened item, none of them checked yet and the first one playing.
    pub fn new(
        item: PlaybackItem,
        links: Vec<PlayableLink>,
        subtitles: Vec<SubtitleTrack>,
    ) -> Self {
        Self {
            health: vec![SourceHealth::Checking; links.len()],
            sizes: vec![None; links.len()],
            item,
            links,
            subtitles,
            selected: 0,
            position_ms: 0,
            generation: 0,
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

    /// Remembers how far playback got, ignoring the zero a failed or loading stream reports.
    pub fn remember_position(&self, position_ms: i64) {
        if position_ms <= 0 {
            return;
        }
        self.with_active(|active| active.position_ms = position_ms);
    }

    /// Records that the stream at `index` is now playing.
    pub fn mark_selected(&self, index: usize) {
        self.with_active(|active| active.selected = index);
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
mod tests {
    use super::*;

    fn sources() -> ActiveSources {
        ActiveSources::new(
            PlaybackItem::default(),
            vec![PlayableLink::default(), PlayableLink::default()],
            Vec::new(),
        )
    }

    #[test]
    fn empty_until_set_and_empty_again_after_clear() {
        let session = SourceSession::default();
        assert!(session.snapshot().is_none());
        session.set(sources());
        assert!(session.snapshot().is_some());
        session.clear();
        assert!(session.snapshot().is_none());
    }

    #[test]
    fn new_sources_start_unchecked_with_the_first_one_playing() {
        let fresh = sources();
        assert_eq!(fresh.health, [SourceHealth::Checking; 2]);
        assert_eq!(fresh.selected, 0);
    }

    #[test]
    fn mark_selected_updates_the_playing_index() {
        let session = SourceSession::default();
        session.set(sources());
        session.mark_selected(1);
        assert_eq!(session.snapshot().map(|active| active.selected), Some(1));
    }

    #[test]
    fn remembered_position_ignores_zero_from_a_failed_stream() {
        let session = SourceSession::default();
        session.set(sources());
        session.remember_position(95_000);
        session.remember_position(0);
        assert_eq!(
            session.snapshot().map(|active| active.position_ms),
            Some(95_000)
        );
    }

    #[test]
    fn sizes_are_kept_for_the_current_generation_only() {
        let session = SourceSession::default();
        let first = session.set(sources());
        assert!(session.set_size(first, 1, 1_400_000_000));
        assert_eq!(
            session.snapshot().map(|active| active.sizes),
            Some(vec![None, Some(1_400_000_000)])
        );
        let second = session.set(sources());
        assert!(!session.set_size(first, 0, 5), "stale result");
        assert!(session.set_size(second, 0, 5));
    }

    #[test]
    fn mark_selected_without_playback_does_nothing() {
        let session = SourceSession::default();
        session.mark_selected(3);
        assert!(session.snapshot().is_none());
    }

    #[test]
    fn health_is_recorded_only_for_the_current_generation() {
        let session = SourceSession::default();
        let first = session.set(sources());
        assert!(session.set_health(first, 0, SourceHealth::Broken));
        let second = session.set(sources());
        assert!(
            !session.set_health(first, 1, SourceHealth::Working),
            "stale result"
        );
        assert!(session.set_health(second, 1, SourceHealth::Working));
        let health = session.snapshot().map(|active| active.health);
        assert_eq!(
            health,
            Some(vec![SourceHealth::Checking, SourceHealth::Working])
        );
        assert!(session.is_current(second) && !session.is_current(first));
        session.clear();
        assert!(!session.is_current(second), "closed");
        assert!(
            !session.set_health(second, 0, SourceHealth::Working),
            "nothing playing"
        );
    }
}
