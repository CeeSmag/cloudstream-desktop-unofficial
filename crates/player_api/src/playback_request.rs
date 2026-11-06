/// An external subtitle file to attach to playback.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubtitleSource {
    /// Subtitle file URL.
    pub url: String,
    /// Language label shown in the player.
    pub language: String,
}

/// The languages of `subtitles` with how many files each has, for log lines: `Arabic=4, English=12`.
pub fn describe_languages(subtitles: &[SubtitleSource]) -> String {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for subtitle in subtitles {
        let language = if subtitle.language.is_empty() {
            "unknown"
        } else {
            subtitle.language.as_str()
        };
        *counts.entry(language).or_default() += 1;
    }
    counts
        .iter()
        .map(|(language, count)| format!("{language}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
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
    /// Normalized 32-character lowercase hex ClearKey CENC decryption key, if the stream is encrypted.
    pub clear_key: Option<String>,
}
