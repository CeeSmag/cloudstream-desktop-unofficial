//! Remembers which streams were found for the title being played, so the player can switch.

use std::sync::{Mutex, PoisonError};

use app_services::PlaybackItem;
use core_model::{PlayableLink, SubtitleTrack};

/// The streams offered for the item that is playing now.
#[derive(Debug, Clone)]
pub struct ActiveSources {
    /// What is being watched; carried into saved progress when the source changes.
    pub item: PlaybackItem,
    /// Playable streams, best first.
    pub links: Vec<PlayableLink>,
    /// External subtitles the provider offered.
    pub subtitles: Vec<SubtitleTrack>,
    /// Index into `links` of the stream that is playing.
    pub selected: usize,
}

/// Tauri-managed holder of the current [`ActiveSources`].
#[derive(Default)]
pub struct SourceSession {
    active: Mutex<Option<ActiveSources>>,
}

impl SourceSession {
    /// Remembers the streams of a newly started item.
    pub fn set(&self, sources: ActiveSources) {
        *self.active.lock().unwrap_or_else(PoisonError::into_inner) = Some(sources);
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

    /// Records that the stream at `index` is now playing.
    pub fn mark_selected(&self, index: usize) {
        if let Some(active) = self
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            active.selected = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources() -> ActiveSources {
        ActiveSources {
            item: PlaybackItem::default(),
            links: vec![PlayableLink::default(), PlayableLink::default()],
            subtitles: Vec::new(),
            selected: 0,
        }
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
    fn mark_selected_updates_the_playing_index() {
        let session = SourceSession::default();
        session.set(sources());
        session.mark_selected(1);
        assert_eq!(session.snapshot().map(|active| active.selected), Some(1));
    }

    #[test]
    fn mark_selected_without_playback_does_nothing() {
        let session = SourceSession::default();
        session.mark_selected(3);
        assert!(session.snapshot().is_none());
    }
}
