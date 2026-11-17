//! How much of the stream mpv keeps in memory, and how far ahead it reads.
//!
//! The byte limits come from the viewer's settings and can change while a stream is loaded
//! (`demuxer-max-bytes` ahead, `demuxer-max-back-bytes` behind). Checked with the real libmpv
//! (`cache_limit_live_tests.rs`): it reads ahead up to a raised limit once playback goes on (not
//! while paused), trims what lies behind to a lowered one at once, but keeps what it already read
//! ahead when that limit is lowered (it only reads no further).
//!
//! How far ahead mpv reads also has a time limit, chosen per stream: a stream split into many
//! small pieces (HLS, DASH) is read at most a minute ahead, since reading it as fast as possible
//! is hundreds of requests and some hosts then refuse them ("HTTP 429"); a plain file is one
//! request, so it may fill the whole byte limit.

use player_api::{CacheLimits, PlaybackRequest, PlayerError, StreamKind};

use crate::mpv_api::MpvHandle;

/// `cache-secs` and `demuxer-readahead-secs` for a stream split into pieces.
const PIECES_READ_AHEAD: (&str, &str) = ("60", "30");
/// The same for a plain file: ten hours, so only the byte limit counts.
const FILE_READ_AHEAD: (&str, &str) = ("36000", "36000");

/// Sets the byte limits of the cache; mpv follows them at once.
pub(crate) fn apply_cache_limits(
    handle: &MpvHandle,
    limits: CacheLimits,
) -> Result<(), PlayerError> {
    handle.set_property("demuxer-max-bytes", &limits.ahead_bytes.to_string())?;
    handle.set_property("demuxer-max-back-bytes", &limits.behind_bytes.to_string())?;
    tracing::info!(
        ahead_mb = limits.ahead_bytes / (1024 * 1024),
        behind_mb = limits.behind_bytes / (1024 * 1024),
        "video cache limits set"
    );
    Ok(())
}

/// Whether the stream comes in many small pieces: HLS or DASH, as the provider said or as the
/// address shows (the provider does not always say).
pub(crate) fn is_in_pieces(request: &PlaybackRequest) -> bool {
    if matches!(request.kind, StreamKind::Hls | StreamKind::Dash) {
        return true;
    }
    let path = request
        .url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    path.ends_with(".m3u8") || path.ends_with(".mpd")
}

/// Sets how far ahead mpv reads the stream that is about to load.
pub(crate) fn apply_read_ahead(
    handle: &MpvHandle,
    request: &PlaybackRequest,
) -> Result<(), PlayerError> {
    let (cache_secs, readahead_secs) = if is_in_pieces(request) {
        PIECES_READ_AHEAD
    } else {
        FILE_READ_AHEAD
    };
    handle.set_property("cache-secs", cache_secs)?;
    handle.set_property("demuxer-readahead-secs", readahead_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(url: &str, kind: StreamKind) -> PlaybackRequest {
        PlaybackRequest {
            url: url.to_owned(),
            kind,
            ..PlaybackRequest::default()
        }
    }

    #[test]
    fn hls_and_dash_come_in_pieces_by_kind_or_by_address() {
        assert!(is_in_pieces(&request("https://h.test/a", StreamKind::Hls)));
        assert!(is_in_pieces(&request("https://h.test/a", StreamKind::Dash)));
        assert!(is_in_pieces(&request(
            "https://h.test/a.M3U8?t=1",
            StreamKind::Unknown
        )));
        assert!(is_in_pieces(&request(
            "https://h.test/m.mpd#x",
            StreamKind::Unknown
        )));
    }

    #[test]
    fn a_plain_file_is_not_in_pieces() {
        assert!(!is_in_pieces(&request(
            "https://h.test/a.mkv",
            StreamKind::Video
        )));
        assert!(!is_in_pieces(&request(
            "https://h.test/a.mp4?x=.m3u8",
            StreamKind::Unknown
        )));
    }
}

#[cfg(test)]
#[path = "cache_limit_live_tests.rs"]
mod live_tests;
