//! A video the viewer left waits for them: leaving the player pauses it, keeps it loaded and
//! shrinks its cache; opening the same video again plays on from memory with the cache back at
//! full size, without a new search. Playing another video or closing the app drops it.
//!
//! Leaving and coming back check and change the open video's list under the opening's lock (the
//! one [`SourceManager::begin`] and [`SourceManager::open`] take), so a video that started opening
//! meanwhile is never paused or resumed in place of the one the viewer left.

use std::sync::PoisonError;

use app_services::PlaybackItem;
use player_api::PlaybackState;

use crate::source_manager::SourceManager;
use crate::source_session::SourceSession;

/// Whether the player still holds a stream that can wait (or play on).
pub(crate) fn holds_a_stream(state: &PlaybackState) -> bool {
    matches!(
        state,
        PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Loading
    )
}

impl SourceSession {
    /// Marks list `generation` as waiting while the viewer is away, or as watched again. Returns
    /// false, and nothing changed, when `generation` is not the open list.
    pub fn set_away(&self, generation: u64, away: bool) -> bool {
        let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        match guard.as_mut() {
            Some(active) if active.generation == generation => {
                active.away = away;
                true
            }
            _ => false,
        }
    }
}

impl SourceManager {
    /// The viewer left the player: when the open video's list is the one the opening opened (no
    /// other video started opening since), marks it waiting and runs `pause` (which pauses and
    /// shrinks the cache). Returns false, and runs nothing, otherwise.
    pub(crate) fn leave_with(
        &self,
        pause: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        let opening = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(generation) = opening.generation else {
            return Ok(false);
        };
        if !self.session().set_away(generation, true) {
            return Ok(false);
        }
        pause()?;
        Ok(true)
    }

    /// The viewer opens `item`: when it is the video that waits, marks it watched again and runs
    /// `resume` (which grows the cache back and plays). Returns false, and runs nothing, when
    /// another video (or none) waits.
    pub(crate) fn come_back_with(
        &self,
        item: &PlaybackItem,
        resume: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        let opening = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(generation) = opening.generation else {
            return Ok(false);
        };
        let waiting = self.session().snapshot().is_some_and(|active| {
            active.generation == generation
                && active.away
                && active.item.provider == item.provider
                && active.item.media_key == item.media_key
        });
        if !waiting || !self.session().set_away(generation, false) {
            return Ok(false);
        }
        resume()?;
        Ok(true)
    }

    /// Runs `apply` with whether the open video waits, under the opening's lock, so a change of
    /// the cache setting is applied with the size that fits what the video is doing.
    pub(crate) fn with_away<T>(&self, apply: impl FnOnce(bool) -> T) -> T {
        let opening = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        let away = opening.generation.is_some_and(|generation| {
            self.session()
                .snapshot()
                .is_some_and(|active| active.generation == generation && active.away)
        });
        apply(away)
    }
}

#[cfg(test)]
#[path = "video_away_tests.rs"]
mod tests;
