//! Decides whether a stream is worth offering, by asking its host for the first bytes.
//!
//! Providers return dozens of links and many are dead (host gone, blocked, a web page instead
//! of a video). This runs only when a video is opened, so the picker lists what can play.

use std::sync::Arc;

use core_model::PlayableLink;
use extension_repo::{HttpProbe, ProbeResponse};

use crate::subtitle_scan::subtitle_languages;
use crate::CheckQueue;

/// How many streams are asked at the same time.
const WORKERS: usize = 12;
/// Sent when a link names no `User-Agent`; mpv itself identifies as libmpv.
const DEFAULT_USER_AGENT: &str = "libmpv";

/// Whether a stream answered like something a player can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkHealth {
    /// The host answered with a video, playlist or manifest.
    Working {
        /// Size of the video file in bytes; unknown for playlists and when the host did not say.
        size_bytes: Option<u64>,
        /// Languages of the subtitles the stream itself carries, as it names them (`eng`, `ar`,
        /// `Arabic`); empty when it has none or says nothing at its start.
        subtitles: Vec<String>,
    },
    /// The host did not answer properly; the reason is fit for a log line.
    Broken(String),
}

impl LinkHealth {
    /// True for [`LinkHealth::Working`].
    pub fn is_working(&self) -> bool {
        matches!(self, Self::Working { .. })
    }
}

/// Probes streams over HTTP.
#[derive(Clone)]
pub struct StreamChecker {
    probe: Arc<dyn HttpProbe>,
}

impl StreamChecker {
    /// Creates a checker that asks through `probe`.
    pub fn new(probe: Arc<dyn HttpProbe>) -> Self {
        Self { probe }
    }

    /// Asks the host of one stream for its first bytes.
    #[tracing::instrument(level = "debug", skip_all, fields(host = link.url.split('/').nth(2).unwrap_or_default()))]
    pub fn check(&self, link: &PlayableLink) -> LinkHealth {
        match self.probe.probe(&link.url, &request_headers(link)) {
            Ok(response) => classify(&link.link_type, &response),
            Err(error) => LinkHealth::Broken(error.to_string()),
        }
    }

    /// Checks the streams that arrive in `queue` with several workers at once and reports each
    /// result as soon as it is known; returns when the queue is closed and empty. `keep_going` is
    /// asked before every new check, so nothing starts once the answers are no longer wanted (a
    /// check already on its way finishes, within the probe time limit); `on_result` returning
    /// `false` says the same from the other side. Whoever stops wanting answers must also
    /// [`CheckQueue::stop`] the queue so idle workers wake up.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check_queue(
        &self,
        queue: &CheckQueue,
        keep_going: &(dyn Fn() -> bool + Sync),
        on_result: &(dyn Fn(usize, LinkHealth) -> bool + Sync),
    ) {
        std::thread::scope(|scope| {
            for _ in 0..WORKERS {
                scope.spawn(|| {
                    while keep_going() {
                        let Some((index, link)) = queue.next() else {
                            break;
                        };
                        if !on_result(index, self.check(&link)) {
                            queue.stop();
                        }
                    }
                });
            }
        });
    }
}

/// The headers the player would send: the link's own, plus its referer and a user agent.
fn request_headers(link: &PlayableLink) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = link
        .headers
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let has = |wanted: &str, list: &[(String, String)]| {
        list.iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(wanted))
    };
    if !link.referer.is_empty() && !has("referer", &headers) {
        headers.push(("Referer".to_owned(), link.referer.clone()));
    }
    if !has("user-agent", &headers) {
        headers.push(("User-Agent".to_owned(), DEFAULT_USER_AGENT.to_owned()));
    }
    headers
}

/// Judges an answer: right status, and a body that looks like media rather than a web page.
pub(crate) fn classify(link_type: &str, response: &ProbeResponse) -> LinkHealth {
    if !(200..300).contains(&response.status) {
        return LinkHealth::Broken(format!("HTTP {}", response.status));
    }
    let head = String::from_utf8_lossy(&response.head);
    let head = head.trim_start();
    if head.starts_with("#EXTM3U") {
        return working(None, response);
    }
    if link_type == "M3U8" {
        return LinkHealth::Broken("not an HLS playlist".to_owned());
    }
    if link_type == "DASH" && (head.starts_with("<?xml") || head.starts_with("<MPD")) {
        return working(None, response);
    }
    if response.head.is_empty() {
        return LinkHealth::Broken("empty answer".to_owned());
    }
    if head.starts_with('<') || response.content_type.contains("text/html") {
        return LinkHealth::Broken("a web page, not a video".to_owned());
    }
    if head.starts_with('{') || head.starts_with('[') {
        return LinkHealth::Broken("data, not a video".to_owned());
    }
    working(
        if link_type == "VIDEO" {
            response.total_bytes
        } else {
            None
        },
        response,
    )
}

fn working(size_bytes: Option<u64>, response: &ProbeResponse) -> LinkHealth {
    LinkHealth::Working {
        size_bytes,
        subtitles: subtitle_languages(&response.head),
    }
}

#[cfg(test)]
#[path = "stream_checker_tests.rs"]
mod tests;
