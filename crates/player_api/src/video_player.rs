use crate::{PlaybackRequest, PlaybackStatus, PlayerError};

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
}
