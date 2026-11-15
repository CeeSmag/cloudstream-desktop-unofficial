use core_model::{PlayableLink, SubtitleTrack};
use player_api::{PlaybackRequest, StreamKind, SubtitleSource};

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
        kind: stream_kind_of(&link.link_type),
        url: link.url,
        title: title.to_owned(),
        headers,
        referer: link.referer,
        subtitles: subtitle_sources(subtitles),
        start_position_ms: start_ms,
        clear_key,
    }
}

/// What a provider's link type (`VIDEO`, `M3U8`, `DASH`) says about the stream.
fn stream_kind_of(link_type: &str) -> StreamKind {
    match link_type {
        "M3U8" => StreamKind::Hls,
        "DASH" => StreamKind::Dash,
        "VIDEO" => StreamKind::Video,
        _ => StreamKind::Unknown,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_of(link_type: &str) -> StreamKind {
        let link = PlayableLink {
            url: "https://h.test/stream".to_owned(),
            link_type: link_type.to_owned(),
            ..PlayableLink::default()
        };
        build_request("title", &link, &[], 0).kind
    }

    #[test]
    fn build_request_takes_the_stream_kind_from_the_link_type() {
        assert_eq!(kind_of("M3U8"), StreamKind::Hls);
        assert_eq!(kind_of("DASH"), StreamKind::Dash);
        assert_eq!(kind_of("VIDEO"), StreamKind::Video);
    }

    #[test]
    fn build_request_does_not_guess_the_kind_of_an_unlabelled_link() {
        assert_eq!(kind_of(""), StreamKind::Unknown);
        assert_eq!(kind_of("TORRENT"), StreamKind::Unknown);
    }
}
