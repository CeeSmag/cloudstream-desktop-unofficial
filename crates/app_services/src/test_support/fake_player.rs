use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use player_api::{CacheLimits, PlaybackRequest, PlaybackStatus, PlayerError, VideoPlayer};

/// A player that records requests and reports a scripted status.
#[derive(Default)]
pub(crate) struct FakePlayer {
    pub(crate) requests: Mutex<Vec<PlaybackRequest>>,
    status: Mutex<PlaybackStatus>,
    pub(crate) verbose_log: AtomicBool,
    /// Pauses and cache limits, in order, as `paused true` / `cache 3 1`.
    pub(crate) calls: Mutex<Vec<String>>,
}

impl FakePlayer {
    pub(crate) fn report(&self, status: PlaybackStatus) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }

    fn note(&self, call: String) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
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

    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        self.note(format!("paused {paused}"));
        Ok(())
    }

    fn set_cache_limits(&self, limits: CacheLimits) -> Result<(), PlayerError> {
        self.note(format!(
            "cache {} {}",
            limits.ahead_bytes, limits.behind_bytes
        ));
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
