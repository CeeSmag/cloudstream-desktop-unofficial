//! Decides whether a stream is worth offering, by asking its host for the first bytes.
//!
//! Providers return dozens of links and many are dead (host gone, blocked, a web page instead
//! of a video). This runs only when a video is opened, so the picker lists what can play.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use core_model::PlayableLink;
use extension_repo::{HttpProbe, ProbeResponse};

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

    /// Checks many streams at once and reports each result as soon as it is known.
    /// `links` pairs each stream with the index the caller knows it by. `keep_going` is asked
    /// before every new check, so nothing starts once the answers are no longer wanted (a check
    /// already on its way finishes, within the probe time limit); `on_result` returning `false`
    /// says the same from the other side.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check_all(
        &self,
        links: &[(usize, PlayableLink)],
        keep_going: &(dyn Fn() -> bool + Sync),
        on_result: &(dyn Fn(usize, LinkHealth) -> bool + Sync),
    ) {
        let next = AtomicUsize::new(0);
        let unwanted = AtomicBool::new(false);
        std::thread::scope(|scope| {
            for _ in 0..WORKERS.min(links.len()) {
                scope.spawn(|| {
                    while !unwanted.load(Ordering::SeqCst) && keep_going() {
                        let position = next.fetch_add(1, Ordering::SeqCst);
                        let Some((index, link)) = links.get(position) else {
                            break;
                        };
                        if !on_result(*index, self.check(link)) {
                            unwanted.store(true, Ordering::SeqCst);
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
        return LinkHealth::Working { size_bytes: None };
    }
    if link_type == "M3U8" {
        return LinkHealth::Broken("not an HLS playlist".to_owned());
    }
    if link_type == "DASH" && (head.starts_with("<?xml") || head.starts_with("<MPD")) {
        return LinkHealth::Working { size_bytes: None };
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
    LinkHealth::Working {
        size_bytes: if link_type == "VIDEO" {
            response.total_bytes
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use extension_repo::RepoError;

    use super::*;

    fn answer(status: u16, content_type: &str, body: &[u8]) -> ProbeResponse {
        ProbeResponse {
            status,
            content_type: content_type.to_owned(),
            head: body.to_vec(),
            total_bytes: None,
        }
    }

    #[test]
    fn a_video_file_is_working() {
        let matroska = [0x1A, 0x45, 0xDF, 0xA3, 0x42];
        assert!(classify("VIDEO", &answer(206, "video/x-matroska", &matroska)).is_working());
    }

    #[test]
    fn a_video_file_reports_its_size_but_a_playlist_does_not() {
        let mut video = answer(206, "video/mp4", b"\0\0\0\x18ftypmp42");
        video.total_bytes = Some(1_400_000_000);
        assert_eq!(
            classify("VIDEO", &video),
            LinkHealth::Working {
                size_bytes: Some(1_400_000_000)
            }
        );
        let mut playlist = answer(200, "application/x-mpegurl", b"#EXTM3U\n");
        playlist.total_bytes = Some(300);
        assert_eq!(
            classify("M3U8", &playlist),
            LinkHealth::Working { size_bytes: None }
        );
    }

    #[test]
    fn an_hls_playlist_is_working_even_when_served_as_html() {
        let playlist = answer(
            200,
            "text/html; charset=UTF-8",
            b"#EXTM3U\n#EXT-X-VERSION:3",
        );
        assert!(classify("M3U8", &playlist).is_working());
    }

    #[test]
    fn error_statuses_are_broken() {
        let health = classify("VIDEO", &answer(403, "", b""));
        assert_eq!(health, LinkHealth::Broken("HTTP 403".to_owned()));
    }

    #[test]
    fn a_web_page_in_place_of_a_video_is_broken() {
        assert!(!classify("VIDEO", &answer(200, "text/html", b"<!doctype html>")).is_working());
        assert!(!classify("VIDEO", &answer(200, "", b"<html>")).is_working());
        assert!(
            !classify("VIDEO", &answer(200, "application/json", b"{\"error\":1}")).is_working()
        );
    }

    #[test]
    fn a_playlist_link_that_is_not_a_playlist_is_broken() {
        assert!(!classify("M3U8", &answer(200, "video/mp4", b"\0\0\0\x18ftypmp42")).is_working());
    }

    #[test]
    fn a_dash_manifest_is_working_and_an_empty_answer_is_not() {
        assert!(classify(
            "DASH",
            &answer(200, "application/dash+xml", b"<?xml version")
        )
        .is_working());
        assert!(!classify("VIDEO", &answer(200, "video/mp4", b"")).is_working());
    }

    #[test]
    fn request_headers_add_referer_and_user_agent_unless_the_link_has_them() {
        let link = PlayableLink {
            referer: "https://site.test".into(),
            headers: HashMap::from([("User-Agent".to_owned(), "MovieBlast".to_owned())]),
            ..PlayableLink::default()
        };
        let headers = request_headers(&link);
        assert!(headers.contains(&("Referer".to_owned(), "https://site.test".to_owned())));
        assert!(headers.contains(&("User-Agent".to_owned(), "MovieBlast".to_owned())));
        assert_eq!(headers.len(), 2);
    }

    #[test]
    fn check_all_stops_early_when_the_answers_are_no_longer_wanted() {
        let checker = StreamChecker::new(Arc::new(ScriptedProbe(HashMap::new())));
        let links: Vec<(usize, PlayableLink)> = (0..200)
            .map(|index| (index, link("https://dead/x")))
            .collect();
        let seen = AtomicUsize::new(0);

        checker.check_all(&links, &|| true, &|_, _| {
            seen.fetch_add(1, Ordering::SeqCst);
            false
        });

        assert!(
            seen.load(Ordering::SeqCst) <= WORKERS,
            "kept going after being told to stop"
        );
    }

    #[test]
    fn check_all_starts_nothing_once_keep_going_says_no() {
        let checker = StreamChecker::new(Arc::new(ScriptedProbe(HashMap::new())));
        let links: Vec<(usize, PlayableLink)> = (0..50)
            .map(|index| (index, link("https://dead/x")))
            .collect();
        let seen = AtomicUsize::new(0);

        checker.check_all(&links, &|| false, &|_, _| {
            seen.fetch_add(1, Ordering::SeqCst);
            true
        });

        assert_eq!(
            seen.load(Ordering::SeqCst),
            0,
            "a check started after cancellation"
        );
    }

    /// Answers by URL; unknown URLs fail like an unreachable host.
    struct ScriptedProbe(HashMap<String, ProbeResponse>);

    impl HttpProbe for ScriptedProbe {
        fn probe(
            &self,
            url: &str,
            _headers: &[(String, String)],
        ) -> Result<ProbeResponse, RepoError> {
            self.0.get(url).cloned().ok_or(RepoError::Download {
                url: url.to_owned(),
                reason: "unreachable".to_owned(),
            })
        }
    }

    fn link(url: &str) -> PlayableLink {
        PlayableLink {
            url: url.to_owned(),
            link_type: "VIDEO".to_owned(),
            ..PlayableLink::default()
        }
    }

    #[test]
    fn check_all_reports_every_stream_under_the_index_the_caller_gave() {
        let probe = ScriptedProbe(HashMap::from([(
            "https://ok/a".to_owned(),
            answer(206, "video/mp4", b"\0\0\0\x18ftyp"),
        )]));
        let checker = StreamChecker::new(Arc::new(probe));
        let links: Vec<(usize, PlayableLink)> = (0..30)
            .map(|index| {
                (
                    index * 2,
                    link(if index == 5 {
                        "https://ok/a"
                    } else {
                        "https://dead/x"
                    }),
                )
            })
            .collect();
        let results = Mutex::new(Vec::new());

        checker.check_all(&links, &|| true, &|index, health| {
            results.lock().expect("lock").push((index, health));
            true
        });

        let mut results = results.into_inner().expect("lock");
        results.sort_by_key(|(index, _)| *index);
        assert_eq!(results.len(), 30);
        let working: Vec<usize> = results
            .iter()
            .filter(|(_, h)| h.is_working())
            .map(|(i, _)| *i)
            .collect();
        assert_eq!(working, [10]);
    }
}
