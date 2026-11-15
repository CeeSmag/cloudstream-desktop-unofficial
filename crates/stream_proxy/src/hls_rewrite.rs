//! Rewrites an HLS playlist so that every address it names is an address of the proxy.
//!
//! Pure text in, text out: the caller supplies the base address the playlist was fetched from and a
//! function that registers an absolute address and returns the proxied one.

use crate::entry_kind::EntryKind;
use crate::resolve_reference::resolve_reference;
use crate::url_parts::is_http;

/// Why a playlist could not be rewritten.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RewriteError {
    /// The body does not start with `#EXTM3U` (an error page, a redirect to a login).
    #[error("the answer is not an HLS playlist")]
    NotAPlaylist,
    /// The playlist names an address that is not `http`, `https` or `data` (scheme given).
    #[error("the playlist names an address with the unsupported scheme {0:?}")]
    UnsupportedAddress(String),
    /// The stream registered more addresses than the proxy keeps.
    #[error("the playlist names more addresses than the proxy keeps")]
    TooManyAddresses,
    /// The address the playlist came from is not an absolute address.
    #[error("the playlist's own address is not absolute")]
    BadBase,
}

/// Registers an absolute address of a kind and returns the proxied one; `None` when it cannot.
pub(crate) type Register<'a> = &'a mut dyn FnMut(&str, EntryKind) -> Option<String>;

/// The kind of address a tag's `URI` attribute holds; `None` for tags whose URI is not media.
fn uri_kind(tag: &str) -> Option<EntryKind> {
    match tag {
        "EXT-X-KEY" | "EXT-X-SESSION-KEY" => Some(EntryKind::Key),
        "EXT-X-MAP" => Some(EntryKind::Init),
        "EXT-X-MEDIA" | "EXT-X-I-FRAME-STREAM-INF" | "EXT-X-RENDITION-REPORT" => {
            Some(EntryKind::Playlist)
        }
        "EXT-X-PART" | "EXT-X-PRELOAD-HINT" => Some(EntryKind::Segment),
        _ => None,
    }
}

/// Where the value of the first `URI="..."` attribute of a tag line starts.
fn uri_value_start(line: &str) -> Option<usize> {
    line.match_indices("URI=\"").find_map(|(index, found)| {
        let before = line.get(..index)?.chars().last()?;
        matches!(before, ':' | ',').then_some(index + found.len())
    })
}

struct Rewriter<'a> {
    base: &'a str,
    register: Register<'a>,
    next_line_is_playlist: bool,
}

impl Rewriter<'_> {
    /// The proxied address for `reference`, found in a playlist fetched from the base.
    fn address(&mut self, reference: &str, kind: EntryKind) -> Result<String, RewriteError> {
        let absolute = resolve_reference(self.base, reference).ok_or(RewriteError::BadBase)?;
        if absolute.starts_with("data:") {
            return Ok(reference.to_owned());
        }
        if !is_http(&absolute) {
            let scheme = absolute.split(':').next().unwrap_or_default().to_owned();
            return Err(RewriteError::UnsupportedAddress(scheme));
        }
        (self.register)(&absolute, kind).ok_or(RewriteError::TooManyAddresses)
    }

    /// A line that is not a tag: the address of a piece or, after a stream tag, of a playlist.
    fn media_line(&mut self, line: &str) -> Result<String, RewriteError> {
        let kind = if std::mem::take(&mut self.next_line_is_playlist) {
            EntryKind::Playlist
        } else {
            let absolute = resolve_reference(self.base, line).unwrap_or_default();
            EntryKind::of_media_line(&absolute)
        };
        self.address(line, kind)
    }

    /// A tag line: its `URI` attribute, when it has one that names media, is rewritten.
    fn tag_line(&mut self, line: &str) -> Result<String, RewriteError> {
        let name = line.trim_start_matches('#').split(':').next().unwrap_or("");
        if name == "EXT-X-STREAM-INF" {
            self.next_line_is_playlist = true;
        }
        let (Some(kind), Some(start)) = (uri_kind(name), uri_value_start(line)) else {
            return Ok(line.to_owned());
        };
        let rest = line.get(start..).unwrap_or("");
        let Some(length) = rest.find('"') else {
            return Ok(line.to_owned());
        };
        let proxied = self.address(rest.get(..length).unwrap_or(""), kind)?;
        let head = line.get(..start).unwrap_or("");
        Ok(format!(
            "{head}{proxied}{}",
            rest.get(length..).unwrap_or("")
        ))
    }

    fn line(&mut self, line: &str) -> Result<String, RewriteError> {
        let line = line.trim_end();
        if line.is_empty() {
            Ok(String::new())
        } else if line.starts_with('#') {
            self.tag_line(line)
        } else {
            self.media_line(line)
        }
    }
}

/// True for a video-on-demand playlist that forgot its end marker; the player would wait for more.
fn lacks_end_marker(text: &str) -> bool {
    text.contains("#EXT-X-PLAYLIST-TYPE:VOD") && !text.contains("#EXT-X-ENDLIST")
}

/// Rewrites `text`, a playlist fetched from `base`: every address in it goes through `register`.
pub(crate) fn rewrite_playlist(
    text: &str,
    base: &str,
    register: Register<'_>,
) -> Result<String, RewriteError> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if !text.starts_with("#EXTM3U") {
        return Err(RewriteError::NotAPlaylist);
    }
    let mut rewriter = Rewriter {
        base,
        register,
        next_line_is_playlist: false,
    };
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    for line in text.lines() {
        out.push_str(&rewriter.line(line)?);
        out.push('\n');
    }
    if lacks_end_marker(text) {
        out.push_str("#EXT-X-ENDLIST\n");
    }
    Ok(out)
}

#[cfg(test)]
#[path = "hls_rewrite_tests.rs"]
mod tests;
