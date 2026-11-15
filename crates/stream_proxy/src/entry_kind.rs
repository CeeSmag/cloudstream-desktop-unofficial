//! What a proxied address stands for, and the file name the player is shown for it.

use crate::url_parts::split_url;

/// What the player will do with an address the proxy handed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    /// A master or media playlist: fetched whole and rewritten.
    Playlist,
    /// A piece of the video, audio or subtitles: passed through as it comes.
    Segment,
    /// An encryption key (`#EXT-X-KEY`): passed through.
    Key,
    /// The initialisation section of fragmented MP4 (`#EXT-X-MAP`): passed through.
    Init,
}

/// Extensions the player's HLS reader accepts for a segment; it refuses any other file name.
const SEGMENT_EXTENSIONS: &[&str] = &[
    "ts", "m4s", "mp4", "m4a", "m4v", "aac", "mp3", "vtt", "webvtt", "cmfv", "cmfa",
];

/// Extension used when a segment's own name is not one the player accepts (providers hide
/// segments behind `.jpg` or `.png`); the content is what counts, not the name.
const FALLBACK_SEGMENT_EXTENSION: &str = "ts";

/// The extension at the end of the path of `url`, in lower case.
fn path_extension(url: &str) -> Option<String> {
    let parts = split_url(url)?;
    let name = parts.path.rsplit('/').next()?;
    let (_, extension) = name.rsplit_once('.')?;
    Some(extension.to_ascii_lowercase())
}

impl EntryKind {
    /// The kind of address a URL on a playlist's media line is.
    pub(crate) fn of_media_line(url: &str) -> Self {
        if path_extension(url).as_deref() == Some("m3u8") {
            Self::Playlist
        } else {
            Self::Segment
        }
    }

    /// The extension the proxied address ends in.
    pub(crate) fn extension(self, original: &str) -> &'static str {
        match self {
            Self::Playlist => "m3u8",
            Self::Key => "key",
            Self::Init => "mp4",
            Self::Segment => path_extension(original)
                .and_then(|own| {
                    SEGMENT_EXTENSIONS
                        .iter()
                        .copied()
                        .find(|known| *known == own)
                })
                .unwrap_or(FALLBACK_SEGMENT_EXTENSION),
        }
    }

    /// The `Content-Type` the proxy answers with: the origin's is often wrong on purpose.
    pub(crate) fn content_type(self) -> &'static str {
        match self {
            Self::Playlist => "application/vnd.apple.mpegurl",
            Self::Segment | Self::Key | Self::Init => "application/octet-stream",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_media_line_playlist_extension_returns_playlist() {
        assert_eq!(
            EntryKind::of_media_line("https://a/b/v.m3u8?t=1"),
            EntryKind::Playlist
        );
        assert_eq!(
            EntryKind::of_media_line("https://a/b/V.M3U8"),
            EntryKind::Playlist
        );
        assert_eq!(
            EntryKind::of_media_line("https://a/b/1.ts"),
            EntryKind::Segment
        );
        assert_eq!(
            EntryKind::of_media_line("https://a/b/noext"),
            EntryKind::Segment
        );
    }

    #[test]
    fn extension_segment_keeps_a_known_name_and_replaces_a_disguised_one() {
        assert_eq!(EntryKind::Segment.extension("https://a/1.m4s"), "m4s");
        assert_eq!(EntryKind::Segment.extension("https://a/1.TS?x=1"), "ts");
        assert_eq!(EntryKind::Segment.extension("https://a/1.jpg"), "ts");
        assert_eq!(EntryKind::Segment.extension("https://a/1"), "ts");
    }

    #[test]
    fn extension_other_kinds_ignore_the_original_name() {
        assert_eq!(EntryKind::Playlist.extension("https://a/x.txt"), "m3u8");
        assert_eq!(EntryKind::Key.extension("https://a/key.bin"), "key");
        assert_eq!(EntryKind::Init.extension("https://a/init.m4s"), "mp4");
    }

    #[test]
    fn content_type_marks_playlists_and_hides_the_rest_as_binary() {
        assert_eq!(
            EntryKind::Playlist.content_type(),
            "application/vnd.apple.mpegurl"
        );
        assert_eq!(
            EntryKind::Segment.content_type(),
            "application/octet-stream"
        );
        assert_eq!(EntryKind::Key.content_type(), "application/octet-stream");
    }
}
