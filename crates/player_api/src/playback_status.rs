/// Coarse playback state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PlaybackState {
    /// Nothing is loaded.
    #[default]
    Idle,
    /// A stream is opening.
    Loading,
    /// Playing.
    Playing,
    /// Paused.
    Paused,
    /// The stream reached its end.
    Ended,
    /// The stream could not be played; carries a readable reason.
    Failed(String),
}

/// A snapshot of the player.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlaybackStatus {
    /// Current state.
    pub state: PlaybackState,
    /// Position in milliseconds.
    pub position_ms: i64,
    /// Stream length in milliseconds, 0 when unknown.
    pub duration_ms: i64,
    /// Whether the stream has a picture. `false` once loaded means audio only (for example an
    /// `.mp3` source), so the app should not keep showing the previous picture.
    /// Whether the stream has a picture. `false` once loaded means audio only (for example an
    /// `.mp3` source), so the app should not keep showing the previous picture.
    pub has_video: bool,
}
