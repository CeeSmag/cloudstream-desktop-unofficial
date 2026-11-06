use core_model::{PlayableLink, SubtitleTrack};
use player_api::{PlaybackRequest, SubtitleSource};

use crate::clear_key::{normalize_clear_key, normalize_pipe_link};

/// The next episode to advance to when current episode finishes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NextEpisode {
    /// Payload identifying the next episode.
    pub media_key: String,
    /// Season number, if known.
    pub season: Option<i32>,
    /// Episode number, if known.
    pub episode: Option<i32>,
}

/// A stream to play, with the subtitles to offer and where to begin.
#[derive(Debug, Clone, Copy)]
pub struct StreamSelection<'a> {
    /// The stream.
    pub link: &'a PlayableLink,
    /// External subtitles the provider offered.
    pub subtitles: &'a [SubtitleTrack],
    /// Where to begin, in milliseconds.
    pub start_position_ms: i64,
}

/// What is being watched; carried into saved progress.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlaybackItem {
    /// Provider the item belongs to.
    pub provider: String,
    /// Payload identifying the item (movie data or episode data).
    pub media_key: String,
    /// Title shown in "continue watching".
    pub title: String,
    /// Provider URL of the parent title.
    pub detail_url: String,
    /// Poster image URL.
    pub poster_url: Option<String>,
    /// Season number, if known.
    pub season: Option<i32>,
    /// Episode number, if known.
    pub episode: Option<i32>,
    /// Next episode, if this show has one.
    pub next_episode: Option<NextEpisode>,
}

/// Converts playable link and subtitle data into an mpv playback request.
pub fn build_request(
    title: &str,
    link: &PlayableLink,
    subtitles: &[SubtitleTrack],
    start_ms: i64,
) -> PlaybackRequest {
    let link = if link.url.contains('|') {
        normalize_pipe_link(link.clone())
    } else {
        link.clone()
    };
    let mut headers: Vec<(String, String)> = link
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    headers.sort();
    let clear_key = normalize_clear_key(link.kid.as_deref(), link.key.as_deref());
    PlaybackRequest {
        url: link.url,
        title: title.to_owned(),
        headers,
        referer: link.referer,
        subtitles: subtitle_sources(subtitles),
        start_position_ms: start_ms,
        clear_key,
    }
}

/// Maps subtitle tracks to player subtitle sources.
pub fn subtitle_sources(subtitles: &[SubtitleTrack]) -> Vec<SubtitleSource> {
    subtitles
        .iter()
        .map(|track| SubtitleSource {
            url: track.url.clone(),
            language: track.lang.clone(),
        })
        .collect()
}
