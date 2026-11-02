/// An external subtitle file to attach to playback.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubtitleSource {
    /// Subtitle file URL.
    pub url: String,
    /// Language label shown in the player.
    pub language: String,
}

/// Everything a player needs to start one stream.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlaybackRequest {
    /// Stream URL.
    pub url: String,
    /// Title shown in the player window.
    pub title: String,
    /// Extra HTTP headers the stream host requires, as (name, value) pairs.
    pub headers: Vec<(String, String)>,
    /// Referer the stream host expects; empty for none.
    pub referer: String,
    /// External subtitles to attach.
    pub subtitles: Vec<SubtitleSource>,
    /// Position to resume from, in milliseconds.
    pub start_position_ms: i64,
}
