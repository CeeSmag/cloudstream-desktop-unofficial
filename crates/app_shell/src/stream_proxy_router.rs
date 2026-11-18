//! Sends HLS and header-bearing progressive streams through the local stream proxy when allowed.

use std::sync::OnceLock;

use app_services::RequestRouter;
use player_api::{PlaybackRequest, StreamKind};
use stream_proxy::{ProxyConfig, StreamProxy, StreamSpec};

/// Whether the viewer allows the proxy; asked before every stream, so the switch applies at once.
type Switch = Box<dyn Fn() -> bool + Send + Sync>;

/// The proxy is started on the first proxied stream, not before: an app that never needs it, or
/// whose viewer switched the proxy off, never opens a port.
pub(crate) struct StreamProxyRouter {
    enabled: Switch,
    proxy: OnceLock<Option<StreamProxy>>,
}

impl StreamProxyRouter {
    pub(crate) fn new(enabled: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            enabled: Box::new(enabled),
            proxy: OnceLock::new(),
        }
    }

    /// The proxy, started now if it was not; `None` when it cannot start (said once in the log).
    fn proxy(&self) -> Option<&StreamProxy> {
        self.proxy
            .get_or_init(|| match StreamProxy::start(ProxyConfig::default()) {
                Ok(proxy) => Some(proxy),
                Err(error) => {
                    tracing::warn!(%error, "the stream proxy cannot start; streams play directly");
                    None
                }
            })
            .as_ref()
    }
}

/// True for a stream the proxy serves: the provider says HLS, or the address is a `.m3u8` file.
/// A DASH stream never goes through it.
pub(crate) fn is_hls(request: &PlaybackRequest) -> bool {
    match request.kind {
        StreamKind::Hls => true,
        StreamKind::Dash => false,
        StreamKind::Video | StreamKind::Unknown => {
            let path = request.url.split(['?', '#']).next().unwrap_or_default();
            path.to_ascii_lowercase().ends_with(".m3u8")
        }
    }
}

/// Whether this stream should go through the host loopback proxy (HLS, or progressive with
/// headers/referer the player may not attach on every request). DASH never does.
pub(crate) fn needs_proxy(request: &PlaybackRequest) -> bool {
    if matches!(request.kind, StreamKind::Dash) {
        return false;
    }
    if is_hls(request) {
        return true;
    }
    !request.headers.is_empty() || !request.referer.is_empty()
}

impl RequestRouter for StreamProxyRouter {
    fn route(&self, request: &PlaybackRequest) -> PlaybackRequest {
        if !(self.enabled)() || !needs_proxy(request) {
            return request.clone();
        }
        let Some(proxy) = self.proxy() else {
            return request.clone();
        };
        let spec = StreamSpec {
            url: request.url.clone(),
            headers: request.headers.clone(),
            referer: request.referer.clone(),
        };
        match proxy.open(&spec) {
            // The headers go to the stream's host through the proxy; the player needs none.
            Ok(url) => PlaybackRequest {
                url,
                headers: Vec::new(),
                referer: String::new(),
                ..request.clone()
            },
            Err(error) => {
                tracing::warn!(%error, "the stream cannot go through the proxy; playing it directly");
                request.clone()
            }
        }
    }

    fn release(&self) {
        if let Some(Some(proxy)) = self.proxy.get() {
            proxy.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(url: &str, kind: StreamKind) -> PlaybackRequest {
        PlaybackRequest {
            url: url.to_owned(),
            kind,
            title: "Episode".to_owned(),
            headers: vec![("X-Token".to_owned(), "t".to_owned())],
            referer: "https://site.test/".to_owned(),
            start_position_ms: 9_000,
            ..PlaybackRequest::default()
        }
    }

    fn plain(url: &str, kind: StreamKind) -> PlaybackRequest {
        PlaybackRequest {
            url: url.to_owned(),
            kind,
            title: "Episode".to_owned(),
            headers: Vec::new(),
            referer: String::new(),
            ..PlaybackRequest::default()
        }
    }

    #[test]
    fn is_hls_follows_the_providers_label_and_falls_back_to_the_address() {
        assert!(is_hls(&request("https://h/x", StreamKind::Hls)));
        assert!(is_hls(&request(
            "https://h/a/master.M3U8?t=1",
            StreamKind::Unknown
        )));
        assert!(is_hls(&request(
            "https://h/a/list.m3u8#x",
            StreamKind::Video
        )));
        assert!(!is_hls(&request(
            "https://h/a/movie.mp4",
            StreamKind::Video
        )));
        assert!(!is_hls(&request("https://h/a/movie", StreamKind::Unknown)));
        assert!(!is_hls(&request("https://h/a/list.m3u8", StreamKind::Dash)));
        assert!(!is_hls(&request(
            "https://h/play?file=x.m3u8",
            StreamKind::Unknown
        )));
    }

    #[test]
    fn needs_proxy_covers_hls_and_header_bearing_progressive_not_dash() {
        assert!(needs_proxy(&request(
            "https://h/a/master.m3u8",
            StreamKind::Hls
        )));
        assert!(needs_proxy(&request(
            "https://h/a/movie.mp4",
            StreamKind::Video
        )));
        assert!(!needs_proxy(&plain(
            "https://h/a/movie.mp4",
            StreamKind::Video
        )));
        assert!(!needs_proxy(&request(
            "https://h/a/manifest.mpd",
            StreamKind::Dash
        )));
    }

    #[test]
    fn route_an_hls_stream_goes_to_the_proxy_without_headers_and_keeps_everything_else() {
        let router = StreamProxyRouter::new(|| true);
        let original = request("https://h.test/a/master.m3u8", StreamKind::Hls);
        let routed = router.route(&original);
        assert!(
            routed.url.starts_with("http://127.0.0.1:"),
            "{}",
            routed.url
        );
        assert!(routed.headers.is_empty() && routed.referer.is_empty());
        assert_eq!(
            (routed.title.as_str(), routed.start_position_ms, routed.kind),
            ("Episode", 9_000, StreamKind::Hls)
        );
        router.release();
    }

    #[test]
    fn route_a_progressive_stream_with_headers_goes_to_the_proxy() {
        let router = StreamProxyRouter::new(|| true);
        let original = request("https://h.test/a/movie.mp4", StreamKind::Video);
        let routed = router.route(&original);
        assert!(
            routed.url.starts_with("http://127.0.0.1:"),
            "{}",
            routed.url
        );
        assert!(routed.headers.is_empty() && routed.referer.is_empty());
        router.release();
    }

    #[test]
    fn route_leaves_the_stream_alone_when_it_is_switched_off_or_plain() {
        let off = StreamProxyRouter::new(|| false);
        let hls = request("https://h.test/a/master.m3u8", StreamKind::Hls);
        assert_eq!(off.route(&hls), hls);
        assert!(
            off.proxy.get().is_none(),
            "switched off: no port was opened"
        );
        let on = StreamProxyRouter::new(|| true);
        assert_eq!(
            on.route(&plain("https://h.test/a/movie.mp4", StreamKind::Video)),
            plain("https://h.test/a/movie.mp4", StreamKind::Video)
        );
        assert_eq!(
            on.route(&request("https://h.test/a/manifest.mpd", StreamKind::Dash)),
            request("https://h.test/a/manifest.mpd", StreamKind::Dash)
        );
        assert!(
            on.proxy.get().is_none(),
            "no proxied stream: no port was opened"
        );
    }

    #[test]
    fn route_the_switch_applies_to_the_next_stream_at_once() {
        let allowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let shared = std::sync::Arc::clone(&allowed);
        let router =
            StreamProxyRouter::new(move || shared.load(std::sync::atomic::Ordering::SeqCst));
        let hls = request("https://h.test/a/master.m3u8", StreamKind::Hls);
        assert_ne!(router.route(&hls), hls);
        allowed.store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(router.route(&hls), hls);
        router.release();
    }

    #[test]
    fn route_a_stream_the_proxy_refuses_plays_directly() {
        let router = StreamProxyRouter::new(|| true);
        let local = request("file:///C:/video.m3u8", StreamKind::Hls);
        assert_eq!(router.route(&local), local);
    }

    #[test]
    fn release_before_any_stream_does_nothing() {
        StreamProxyRouter::new(|| true).release();
    }
}

#[cfg(test)]
#[path = "stream_proxy_live_tests.rs"]
mod live_tests;
