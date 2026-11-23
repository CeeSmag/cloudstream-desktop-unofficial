//! Answers a request for a playlist: reads it whole, rewrites it, and answers the player's range
//! over the rewritten text.

use std::io::{Read, Write};

use crate::byte_range::{answer_range, RangeAnswer};
use crate::entry_kind::EntryKind;
use crate::hls_rewrite::rewrite_playlist;
use crate::http_response::{write_head, write_message};
use crate::origin_fetch::OriginResponse;
use crate::proxy_url::proxied_url;
use crate::stream_session::StreamSession;

/// How much is read before deciding whether the answer is a playlist.
const SNIFF_BYTES: u64 = 1024;

/// A playlist longer than this is not a playlist (a full-day live stream is a few hundred KB).
pub(crate) const MAX_PLAYLIST_BYTES: u64 = 8 * 1024 * 1024;

/// Where an answered playlist's addresses are registered, and what was asked of it.
pub(crate) struct PlaylistJob<'a> {
    pub(crate) session: &'a StreamSession,
    pub(crate) port: u16,
    pub(crate) head_only: bool,
    /// The player's `Range` header. The player asks for `bytes=0-` on every request, playlists
    /// included, and the answer must be a range of the *rewritten* text.
    pub(crate) range: Option<&'a str>,
}

/// True when the bytes start like an HLS playlist (after a byte order mark and spaces).
pub(crate) fn looks_like_playlist(prefix: &[u8]) -> bool {
    let text = prefix.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(prefix);
    let start = text.iter().position(|byte| !byte.is_ascii_whitespace());
    start
        .and_then(|start| text.get(start..))
        .is_some_and(|rest| rest.starts_with(b"#EXTM3U"))
}

fn read_up_to(body: &mut dyn Read, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    body.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The first bytes of an answer, enough to tell whether it is a playlist.
pub(crate) fn sniff(response: &mut OriginResponse) -> std::io::Result<Vec<u8>> {
    read_up_to(&mut *response.body, SNIFF_BYTES)
}

/// Answers with the playlist rewritten to point at the proxy. `first` is what `sniff` took from
/// the body; the rest is read here.
pub(crate) fn answer_playlist(
    out: &mut impl Write,
    job: &PlaylistJob<'_>,
    mut response: OriginResponse,
    first: Vec<u8>,
) -> std::io::Result<()> {
    let mut bytes = first;
    match read_up_to(&mut *response.body, MAX_PLAYLIST_BYTES) {
        Ok(rest) if (rest.len() as u64) < MAX_PLAYLIST_BYTES => bytes.extend(rest),
        _ => return write_message(out, 502, "The stream's playlist could not be read."),
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut register = |url: &str, kind: EntryKind| {
        let id = job.session.register(url, kind)?;
        Some(proxied_url(
            job.port,
            job.session.token(),
            id,
            kind.extension(url),
        ))
    };
    match rewrite_playlist(&text, &response.final_url, &mut register) {
        Ok(rewritten) => send_range_of(out, job, rewritten.as_bytes()),
        Err(error) => {
            tracing::warn!(%error, "a playlist of the stream could not be used");
            write_message(out, 502, "The stream's playlist could not be used.")
        }
    }
}

/// Writes the rewritten playlist, or the part of it the player asked for.
fn send_range_of(
    out: &mut impl Write,
    job: &PlaylistJob<'_>,
    playlist: &[u8],
) -> std::io::Result<()> {
    let total = playlist.len();
    let content_type = EntryKind::Playlist.content_type().to_owned();
    let mut headers = vec![
        ("Content-Type", content_type),
        ("Cache-Control", "no-store".to_owned()),
    ];
    let (status, body) = match answer_range(job.range, total) {
        RangeAnswer::Whole => (200, playlist),
        RangeAnswer::Partial { start, end } => {
            headers.push(("Content-Range", format!("bytes {start}-{end}/{total}")));
            (206, playlist.get(start..=end).unwrap_or_default())
        }
        RangeAnswer::Unsatisfiable => {
            headers.push(("Content-Range", format!("bytes */{total}")));
            return write_head(out, 416, &headers);
        }
    };
    headers.push(("Content-Length", body.len().to_string()));
    write_head(out, status, &headers)?;
    if job.head_only {
        return Ok(());
    }
    out.write_all(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_playlist_accepts_the_header_after_a_mark_and_spaces() {
        assert!(looks_like_playlist(b"#EXTM3U\n#EXT-X-VERSION:3"));
        assert!(looks_like_playlist(b"\xEF\xBB\xBF\r\n  #EXTM3U"));
    }

    #[test]
    fn looks_like_playlist_refuses_pages_files_and_nothing() {
        assert!(!looks_like_playlist(b"<html>"));
        assert!(!looks_like_playlist(b"\x00\x00\x00\x18ftypmp42"));
        assert!(!looks_like_playlist(b""));
        assert!(!looks_like_playlist(b"   "));
        assert!(!looks_like_playlist(b"EXTM3U"));
    }
}
