use std::sync::Arc;

use player_api::{
    Chapter, Language, MediaTrack, PlaybackRequest, PlaybackStatus, PlayerError, SubtitleSource,
    TrackKind, VideoFrame, VideoPlayer,
};

use crate::request_router::RequestRouter;

/// A player that lets a [`RequestRouter`] change what it opens, and does everything else exactly
/// like the player it wraps.
///
/// Every method of [`VideoPlayer`] is forwarded below, the ones that have a default body too:
/// when a method is added to the trait, add it here as well (the test in `routed_player_tests.rs`
/// lists them all).
pub struct RoutedPlayer {
    inner: Arc<dyn VideoPlayer>,
    router: Arc<dyn RequestRouter>,
}

impl RoutedPlayer {
    /// Wraps `inner`; `router` sees every request before it is played.
    pub fn new(inner: Arc<dyn VideoPlayer>, router: Arc<dyn RequestRouter>) -> Self {
        Self { inner, router }
    }
}

impl VideoPlayer for RoutedPlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        let routed = self.router.route(request);
        let result = self.inner.play(&routed);
        if result.is_err() {
            self.router.release();
        }
        result
    }

    /// The player stops first: letting go of the stream while the player still reads it would
    /// make that read fail, and a failed read looks like a broken source.
    fn stop(&self) -> Result<(), PlayerError> {
        let result = self.inner.stop();
        self.router.release();
        result
    }

    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        self.inner.set_paused(paused)
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        self.inner.seek_to(position_ms)
    }

    fn set_volume(&self, percent: u8) -> Result<(), PlayerError> {
        self.inner.set_volume(percent)
    }

    fn status(&self) -> PlaybackStatus {
        self.inner.status()
    }

    fn tracks(&self) -> Vec<MediaTrack> {
        self.inner.tracks()
    }

    fn select_track(&self, kind: TrackKind, id: Option<i64>) -> Result<(), PlayerError> {
        self.inner.select_track(kind, id)
    }

    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        self.inner.add_subtitles(subtitles);
    }

    fn set_preferred_audio(&self, language: Option<Language>) {
        self.inner.set_preferred_audio(language);
    }

    fn set_verbose_log(&self, enabled: bool) {
        self.inner.set_verbose_log(enabled);
    }

    fn set_preferred_subtitles(&self, languages: &[Language]) {
        self.inner.set_preferred_subtitles(languages);
    }

    fn chapters(&self) -> Vec<Chapter> {
        self.inner.chapters()
    }

    fn is_embedded(&self) -> bool {
        self.inner.is_embedded()
    }

    fn set_surface_size(&self, width: u32, height: u32) {
        self.inner.set_surface_size(width, height);
    }

    fn latest_frame(&self, last_seen_serial: u64) -> Option<Arc<VideoFrame>> {
        self.inner.latest_frame(last_seen_serial)
    }
}

#[cfg(test)]
#[path = "routed_player_tests.rs"]
mod tests;
