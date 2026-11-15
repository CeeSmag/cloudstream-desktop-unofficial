//! Answers a request for a playlist: rewrites it, or passes on what it really is.

use std::io::{Read, Write};

use crate::entry_kind::EntryKind;
use crate::hls_rewrite::rewrite_playlist;
use crate::http_response::{write_head, write_message};
use crate::origin_fetch::OriginResponse;
use crate::proxy_url::proxied_url;
use crate::relay_body::{relay, RelayJob};
use crate::stream_session::StreamSession;

/// How much is read before deciding whether the answer is a playlist.
const SNIFF_BYTES: u64 = 1024;

/// A playlist longer than this is not a playlist (a full-day live stream is a few hundred KB).
const MAX_PLAYLIST_BYTES: u64 = 8 * 1024 * 1024;

/// Where an answered playlist's addresses are registered and what they will look like.
pub(crate) struct PlaylistJob<'a> {
    pub(crate) session: &'a StreamSession,
    pub(crate) port: u16,
    pub(crate) head_only: bool,
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

/// Answers with the playlist rewritten to point at the proxy. Something that turns out not to be
/// a playlist (a link labelled HLS that is a plain file) is passed on as it is.
pub(crate) fn answer_playlist(
    out: &mut impl Write,
    job: &PlaylistJob<'_>,
    mut response: OriginResponse,
) -> std::io::Result<()> {
    let relay_job = RelayJob {
        session: job.session,
        kind: EntryKind::Playlist,
        head_only: job.head_only,
    };
    let Ok(mut bytes) = read_up_to(&mut *response.body, SNIFF_BYTES) else {
        return write_message(out, 502, "The stream's host stopped answering.");
    };
    if !looks_like_playlist(&bytes) {
        tracing::debug!("the address is not an HLS playlist; passing it on as it is");
        return relay(out, &relay_job, response, &bytes);
    }
    match read_up_to(&mut *response.body, MAX_PLAYLIST_BYTES) {
        Ok(rest) if (rest.len() as u64) < MAX_PLAYLIST_BYTES => bytes.extend(rest),
        _ => return write_message(out, 502, "The stream's playlist could not be read."),
    }
    send_rewritten(out, job, &response.final_url, &bytes)
}

fn send_rewritten(
    out: &mut impl Write,
    job: &PlaylistJob<'_>,
    base: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let text = String::from_utf8_lossy(bytes);
    let mut register = |url: &str, kind: EntryKind| {
        let id = job.session.register(url, kind)?;
        Some(proxied_url(
            job.port,
            job.session.token(),
            id,
            kind.extension(url),
        ))
    };
    match rewrite_playlist(&text, base, &mut register) {
        Ok(rewritten) => {
            let headers = [
                (
                    "Content-Type",
                    EntryKind::Playlist.content_type().to_owned(),
                ),
                ("Content-Length", rewritten.len().to_string()),
                ("Cache-Control", "no-store".to_owned()),
            ];
            write_head(out, 200, &headers)?;
            if job.head_only {
                return Ok(());
            }
            out.write_all(rewritten.as_bytes())
        }
        Err(error) => {
            tracing::warn!(%error, "a playlist of the stream could not be used");
            write_message(out, 502, "The stream's playlist could not be used.")
        }
    }
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
