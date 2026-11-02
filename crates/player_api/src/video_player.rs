use std::sync::Arc;

use crate::{PlaybackRequest, PlaybackStatus, PlayerError, VideoFrame};

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

    /// Returns the latest playback snapshot.
    fn status(&self) -> PlaybackStatus;

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
