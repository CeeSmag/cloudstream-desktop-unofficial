use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use player_api::{PlaybackRequest, PlaybackStatus, PlayerError, VideoPlayer};

/// A player that records requests and reports a scripted status.
#[derive(Default)]
pub(crate) struct FakePlayer {
    pub(crate) requests: Mutex<Vec<PlaybackRequest>>,
    status: Mutex<PlaybackStatus>,
    pub(crate) verbose_log: AtomicBool,
}

impl FakePlayer {
    pub(crate) fn report(&self, status: PlaybackStatus) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }
}

impl VideoPlayer for FakePlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        Ok(())
    }

    fn set_paused(&self, _paused: bool) -> Result<(), PlayerError> {
        Ok(())
    }

    fn seek_to(&self, _position_ms: i64) -> Result<(), PlayerError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlayerError> {
        Ok(())
    }

    fn set_verbose_log(&self, enabled: bool) {
        self.verbose_log.store(enabled, Ordering::SeqCst);
    }

    fn status(&self) -> PlaybackStatus {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
