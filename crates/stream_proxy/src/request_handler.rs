//! Decides what to do with one request from the player.

use std::io::Write;
use std::sync::Arc;

use crate::entry_kind::EntryKind;
use crate::http_request::HttpRequest;
use crate::http_response::write_message;
use crate::origin_fetch::{fetch, OriginResponse};
use crate::playlist_response::{answer_playlist, looks_like_playlist, sniff, PlaylistJob};
use crate::proxy_url::parse_path;
use crate::relay_body::{relay, RelayJob};
use crate::session_registry::SessionRegistry;
use crate::stream_session::StreamSession;
use crate::upstream::Upstream;
use crate::url_parts::host_of;
use crate::url_table::Entry;

/// Why a request is not served, and what the player is told.
struct Refusal {
    status: u16,
    message: &'static str,
}

/// A request that passed the checks: which stream, which address, what was asked of it.
struct Target {
    session: Arc<StreamSession>,
    entry: Entry,
    range: Option<String>,
    head_only: bool,
}

/// Answers the player's requests from the open streams.
pub(crate) struct Handler {
    registry: Arc<SessionRegistry>,
    upstream: Arc<dyn Upstream>,
    port: u16,
}

impl Handler {
    pub(crate) fn new(
        registry: Arc<SessionRegistry>,
        upstream: Arc<dyn Upstream>,
        port: u16,
    ) -> Self {
        Self {
            registry,
            upstream,
            port,
        }
    }

    /// Writes the answer to `request`.
    pub(crate) fn serve(&self, request: &HttpRequest, out: &mut impl Write) -> std::io::Result<()> {
        tracing::debug!(
            method = %request.method,
            target = %request.target,
            range = ?request.header("range"),
            "a request to the stream proxy"
        );
        match self.target_of(request) {
            Ok(target) => self.answer(&target, out),
            Err(refusal) => {
                tracing::debug!(status = refusal.status, "the request is refused");
                write_message(out, refusal.status, refusal.message)
            }
        }
    }

    /// Only this PC's own name for the proxy is served: a web page cannot reach it through a name
    /// that points at it (DNS rebinding).
    fn is_own_host(&self, host: Option<&str>) -> bool {
        let port = self.port;
        host.is_some_and(|host| {
            host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
        })
    }

    fn target_of(&self, request: &HttpRequest) -> Result<Target, Refusal> {
        let refuse = |status, message| Refusal { status, message };
        if request.method != "GET" && request.method != "HEAD" {
            return Err(refuse(405, "Only GET and HEAD are served."));
        }
        if !self.is_own_host(request.header("host")) {
            return Err(refuse(403, "Not served under this name."));
        }
        let path = parse_path(&request.target).ok_or(refuse(404, "No such address."))?;
        let session = self
            .registry
            .lookup(&path.token)
            .ok_or(refuse(410, "This stream is no longer open."))?;
        let entry = session
            .entry(path.id)
            .ok_or(refuse(404, "No such address."))?;
        Ok(Target {
            session,
            entry,
            range: request.header("range").map(str::to_owned),
            head_only: request.method == "HEAD",
        })
    }

    /// Asks the stream's host; a failure is logged (host only, never the whole address).
    fn fetch_logged(&self, target: &Target, range: Option<&str>) -> Option<OriginResponse> {
        let fetched = fetch(
            &*self.upstream,
            target.session.headers(),
            &target.entry.url,
            range,
        );
        match fetched {
            Ok(response) => {
                tracing::debug!(
                    status = response.status,
                    kind = ?target.entry.kind,
                    "the stream's host answered"
                );
                Some(response)
            }
            Err(error) => {
                let host = host_of(&target.entry.url).unwrap_or_default();
                tracing::warn!(%error, %host, "the stream's host did not answer");
                None
            }
        }
    }

    fn answer(&self, target: &Target, out: &mut impl Write) -> std::io::Result<()> {
        let playlist = target.entry.kind == EntryKind::Playlist;
        // A playlist is always read whole, whatever the player asks: the player asks for
        // `bytes=0-` on every request, playlists included, and what it needs is that range of the
        // *rewritten* text, not of the host's.
        let range = if playlist {
            None
        } else {
            target.range.as_deref()
        };
        let Some(response) = self.fetch_logged(target, range) else {
            return write_message(out, 502, "The stream's host did not answer.");
        };
        if target.session.is_revoked() {
            return write_message(out, 410, "This stream is no longer open.");
        }
        if playlist && (200..300).contains(&response.status) {
            return self.answer_playlist_entry(target, response, out);
        }
        relay(out, &relay_job(target), response, &[])
    }

    /// An address that should be a playlist. One that turns out to be something else (a link
    /// labelled HLS that is a plain file) is passed on as it is, ranges included.
    fn answer_playlist_entry(
        &self,
        target: &Target,
        mut response: OriginResponse,
        out: &mut impl Write,
    ) -> std::io::Result<()> {
        let Ok(first) = sniff(&mut response) else {
            return write_message(out, 502, "The stream's host stopped answering.");
        };
        if looks_like_playlist(&first) {
            let job = PlaylistJob {
                session: &target.session,
                port: self.port,
                head_only: target.head_only,
                range: target.range.as_deref(),
            };
            return answer_playlist(out, &job, response, first);
        }
        tracing::debug!("the address is not an HLS playlist; passing it on as it is");
        let Some(range) = target.range.as_deref() else {
            return relay(out, &relay_job(target), response, &first);
        };
        drop(response);
        match self.fetch_logged(target, Some(range)) {
            Some(ranged) => relay(out, &relay_job(target), ranged, &[]),
            None => write_message(out, 502, "The stream's host did not answer."),
        }
    }
}

fn relay_job(target: &Target) -> RelayJob<'_> {
    RelayJob {
        session: &target.session,
        kind: target.entry.kind,
        head_only: target.head_only,
    }
}
