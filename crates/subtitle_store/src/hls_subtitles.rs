//! Some providers offer a subtitle as an HLS playlist of small WebVTT pieces. mpv cannot show that
//! as a subtitle file, so the pieces are downloaded and joined into one WebVTT file.

use crate::subtitle_fetcher::SubtitleFetcher;

/// The most pieces joined; a film's subtitles never need more.
const MAX_SEGMENTS: usize = 4_000;
const HEADER: &str = "WEBVTT\n\n";

/// Resolves `reference` (a relative or absolute address in a playlist) against the playlist's own.
pub(crate) fn resolve_url(base: &str, reference: &str) -> String {
    if reference.contains("://") {
        return reference.to_owned();
    }
    let (scheme_and_host, path) = match base.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('/') {
            Some((host, path)) => (format!("{scheme}://{host}"), format!("/{path}")),
            None => (format!("{scheme}://{rest}"), "/".to_owned()),
        },
        None => (String::new(), base.to_owned()),
    };
    if reference.starts_with('/') {
        return format!("{scheme_and_host}{reference}");
    }
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    format!("{scheme_and_host}{folder}/{reference}")
}

fn media_lines(playlist: &str) -> Vec<&str> {
    playlist
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// The address a master playlist points at for its subtitles.
fn subtitle_rendition(playlist: &str) -> Option<String> {
    let media = playlist
        .lines()
        .find(|line| line.trim().starts_with("#EXT-X-MEDIA:TYPE=SUBTITLES"))
        .and_then(|line| {
            let after = line.split_once("URI=\"")?.1;
            Some(after.split_once('"')?.0.to_owned())
        });
    media.or_else(|| {
        playlist
            .contains("#EXT-X-STREAM-INF")
            .then(|| media_lines(playlist).first().map(|line| (*line).to_owned()))
            .flatten()
    })
}

/// Joins the WebVTT pieces of a subtitle playlist into one file; `None` when no piece had cues.
pub(crate) fn flatten_playlist(
    playlist: &str,
    playlist_url: &str,
    fetcher: &dyn SubtitleFetcher,
) -> Option<String> {
    let (mut text, mut base) = (playlist.to_owned(), playlist_url.to_owned());
    if let Some(child) = subtitle_rendition(playlist) {
        let child_url = resolve_url(playlist_url, &child);
        let body = fetcher.fetch(&child_url, &[]).ok()?;
        text = String::from_utf8_lossy(&body).into_owned();
        base = child_url;
    }
    let mut joined = String::from(HEADER);
    let mut has_cues = false;
    for line in media_lines(&text).into_iter().take(MAX_SEGMENTS) {
        let Ok(body) = fetcher.fetch(&resolve_url(&base, line), &[]) else {
            tracing::warn!(url = %base, "a piece of a subtitle playlist could not be downloaded");
            continue;
        };
        for cue_line in String::from_utf8_lossy(&body)
            .trim_start_matches('\u{FEFF}')
            .lines()
        {
            let trimmed = cue_line.trim();
            if trimmed == "WEBVTT"
                || trimmed.starts_with("X-TIMESTAMP-MAP")
                || trimmed.starts_with("NOTE")
            {
                continue;
            }
            joined.push_str(cue_line);
            joined.push('\n');
            has_cues = true;
        }
        joined.push('\n');
    }
    has_cues.then_some(joined)
}

#[cfg(test)]
#[path = "hls_subtitles_tests.rs"]
mod tests;
