//! A video that waits while the viewer is away from the player: it stays loaded and paused with a
//! smaller cache, so coming back to it plays on from memory instead of opening it again.

use std::sync::PoisonError;

use player_api::CacheLimits;

use crate::{PlaybackService, ServiceError};

impl PlaybackService {
    /// Sets how much of the stream the player keeps in memory.
    pub fn set_cache_limits(&self, limits: CacheLimits) -> Result<(), ServiceError> {
        Ok(self.player.set_cache_limits(limits)?)
    }

    /// The viewer left the player: the progress is saved now, the video pauses and its cache
    /// shrinks to `away`. The stream stays loaded for [`PlaybackService::come_back`].
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn leave(&self, away: CacheLimits) -> Result<(), ServiceError> {
        let status = self.player.status();
        if let Some(current) = self
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            if let Err(error) = self.save_progress(current, &status) {
                tracing::warn!(%error, "could not save watch progress on leaving the player");
            }
        }
        self.player.set_paused(true)?;
        self.player.set_cache_limits(away)?;
        tracing::info!(
            position_ms = status.position_ms,
            "the video waits while the viewer is away"
        );
        Ok(())
    }

    /// The viewer came back to the waiting video: the cache grows back to `watching` and it plays.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn come_back(&self, watching: CacheLimits) -> Result<(), ServiceError> {
        self.player.set_cache_limits(watching)?;
        self.player.set_paused(false)?;
        tracing::info!("the viewer came back; the waiting video plays on");
        Ok(())
    }
}

#[cfg(test)]
#[path = "playback_away_tests.rs"]
mod tests;
