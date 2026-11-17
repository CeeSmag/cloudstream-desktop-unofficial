//! Starting the list of the playing video over, for a refresh of its sources: only the stream that
//! plays stays, so playback goes on while the provider searches again.

use std::sync::atomic::Ordering;
use std::sync::PoisonError;

use crate::source_session::{ActiveSources, SourceHealth, SourceSession};

impl ActiveSources {
    /// The same video with only the stream that plays: selected, playing and known to work, with
    /// what the check learned about it and the subtitles the player already has. `None` when no
    /// stream plays.
    fn keeping_playing(&self) -> Option<Self> {
        if !self.playing {
            return None;
        }
        let link = self.links.get(self.selected)?.clone();
        let mut fresh = Self::new(self.item.clone(), vec![link], self.subtitles.clone());
        fresh.health = vec![SourceHealth::Working];
        fresh.sizes = vec![self.sizes.get(self.selected).copied().flatten()];
        fresh.stream_subtitles = vec![self
            .stream_subtitles
            .get(self.selected)
            .cloned()
            .unwrap_or_default()];
        fresh.position_ms = self.position_ms;
        fresh.subtitles_sent = self.subtitles_sent;
        fresh.playing = true;
        Some(fresh)
    }
}

impl SourceSession {
    /// Replaces the list of the open video by one that holds only the stream that plays (see
    /// [`ActiveSources::keeping_playing`]) and returns its new generation, so the work of the old
    /// list stops. `None`, and nothing changed, when `generation` is no longer the open video or no
    /// stream plays (it is starting or switching). The check and the change happen under one lock.
    pub fn restart_playing(&self, generation: u64) -> Option<u64> {
        let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let mut fresh = guard
            .as_ref()
            .filter(|active| active.generation == generation)?
            .keeping_playing()?;
        let next = self.generations.fetch_add(1, Ordering::SeqCst) + 1;
        fresh.generation = next;
        *guard = Some(fresh);
        Some(next)
    }
}
