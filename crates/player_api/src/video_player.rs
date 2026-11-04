use std::sync::Arc;

use crate::{
    Language, MediaTrack, PlaybackRequest, PlaybackStatus, PlayerError, SubtitleSource, TrackKind,
    VideoFrame,
};

/// A video player the app can drive.
pub trait VideoPlayer: Send + Sync {
    /// Starts playing a stream, replacing whatever was playing.
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError>;

    /// Pauses or resumes playback.
    fn set_paused(&self, paused: bool) -> Result<(), PlayerError>;

    /// Seeks to an absolute position in milliseconds.
    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError>;

    /// Stops playback and closes the stream.
    fn stop(&self) -> Result<(), PlayerError>;

    /// Sets the output volume as a percentage (0 is silent, 100 is the stream's own level).
    fn set_volume(&self, percent: u8) -> Result<(), PlayerError> {
        let _ = percent;
        Ok(())
    }

    /// Returns the latest playback snapshot.
    fn status(&self) -> PlaybackStatus;

    /// The audio and subtitle tracks of the current stream (empty until it has loaded).
    fn tracks(&self) -> Vec<MediaTrack> {
        Vec::new()
    }

    /// Selects a track of `kind` by id, or turns that kind off with `None` (subtitles only make
    /// sense off; turning audio off mutes the stream).
    fn select_track(&self, kind: TrackKind, id: Option<i64>) -> Result<(), PlayerError> {
        let _ = (kind, id);
        Ok(())
    }

    /// Offers subtitles that were found after the stream started; they are attached to the
    /// stream that is playing (or loading) and shown in its track list.
    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        let _ = subtitles;
    }

    /// Sets the language whose audio track is played when a stream offers it; `None` leaves the
    /// choice to the stream. Takes effect from the next stream that loads.
    fn set_preferred_audio(&self, language: Option<Language>) {
        let _ = language;
    }

    /// Makes the player log everything it does (its own detailed messages) while `enabled`, and only
    /// its warnings and errors otherwise. For tracing odd behavior; takes effect at once.
    fn set_verbose_log(&self, enabled: bool) {
        let _ = enabled;
    }

    /// Sets the subtitle languages, in order of preference, whose subtitles are turned on by
    /// themselves once a stream has one; an empty list turns that off. A subtitle the viewer picks
    /// by hand is never overridden.
    fn set_preferred_subtitles(&self, languages: &[Language]) {
        let _ = languages;
    }

    /// Whether pictures are delivered through [`VideoPlayer::latest_frame`] (embedded in the
    /// app window) instead of a separate player window.
    fn is_embedded(&self) -> bool {
        false
    }

    /// Tells an embedded player how large the picture area is, in physical pixels.
    fn set_surface_size(&self, width: u32, height: u32) {
        let _ = (width, height);
    }

    /// The newest picture, unless its serial equals `last_seen_serial`.
    fn latest_frame(&self, last_seen_serial: u64) -> Option<Arc<VideoFrame>> {
        let _ = last_seen_serial;
        None
    }
}
