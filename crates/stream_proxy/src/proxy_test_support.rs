//! Test support for the T2 tests: a stream that wants the provider's headers on every request, and
//! the checks every test of the proxy shares.

use std::sync::Arc;

use crate::fake_origin::{Answer, FakeOrigin, Reply};
use crate::http_request::HttpRequest;
use crate::ureq_upstream::UreqUpstream;
use crate::{ProxyConfig, StreamProxy, StreamSpec};

pub(crate) const REFERER: &str = "https://site.test/watch";
pub(crate) const KEY_BYTES: &[u8] = b"0123456789abcdef";

pub(crate) fn proxy() -> StreamProxy {
    StreamProxy::start_with(
        ProxyConfig::default(),
        Arc::new(UreqUpstream::without_proxy()),
    )
    .expect("the proxy starts")
}

pub(crate) fn spec(url: String) -> StreamSpec {
    StreamSpec {
        url,
        headers: vec![("X-Token".to_owned(), "t".to_owned())],
        referer: REFERER.to_owned(),
    }
}

/// The non-comment lines of a playlist.
pub(crate) fn media_lines(playlist: &str) -> Vec<String> {
    playlist
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The value of the `URI="..."` attribute on the first line that starts with `tag`.
pub(crate) fn uri_of(playlist: &str, tag: &str) -> String {
    let line = playlist
        .lines()
        .find(|line| line.starts_with(tag))
        .expect("the tag is there");
    let rest = line.split("URI=\"").nth(1).expect("a URI attribute");
    rest.split('"').next().expect("a closing quote").to_owned()
}

/// A host that wants the provider's headers on every request, like the ones the proxy is for.
pub(crate) fn strict_site(request: &HttpRequest) -> Option<Answer> {
    let allowed =
        request.header("x-token") == Some("t") && request.header("referer") == Some(REFERER);
    (!allowed).then(|| Answer::status(403))
}

pub(crate) fn show(request: &HttpRequest) -> Answer {
    if let Some(denied) = strict_site(request) {
        return denied;
    }
    match request.target.as_str() {
        "/show/master.m3u8" => {
            Answer::ok("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000\nv/index.m3u8\n")
        }
        "/show/v/index.m3u8" => Answer::ok(
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-KEY:METHOD=AES-128,URI=\"../key.bin\"\n\
#EXTINF:4,\nseg1.jpg\n#EXTINF:4,\nseg2.jpg\n#EXT-X-ENDLIST\n",
        ),
        "/show/key.bin" => Answer::ok(KEY_BYTES),
        "/show/v/seg1.jpg" => Answer::ok("SEGMENT-ONE"),
        "/show/v/seg2.jpg" => Answer::ok("SEGMENT-TWO"),
        _ => Answer::status(404),
    }
}

/// What the player is told about the master playlist: every address in it is the proxy's.
pub(crate) fn assert_master_is_rewritten(master: &Reply, proxy: &StreamProxy) -> Vec<String> {
    assert_eq!(master.status, 200);
    assert_eq!(
        master.header("content-type"),
        Some("application/vnd.apple.mpegurl")
    );
    let prefix = format!("http://127.0.0.1:{}/s/", proxy.port());
    let variants = media_lines(&master.text());
    assert!(
        variants.iter().all(|line| line.starts_with(&prefix)),
        "{variants:?}"
    );
    assert!(
        !master.text().contains("show"),
        "no part of the real address is left"
    );
    variants
}

/// Every request the stream's host saw carried the provider's headers.
pub(crate) fn assert_provider_headers_everywhere(origin: &FakeOrigin) {
    let seen = origin.requests();
    assert_eq!(seen.len(), 5, "master, variant, key and two pieces");
    for request in &seen {
        let target = &request.target;
        assert_eq!(request.header("x-token"), Some("t"), "{target}");
        assert_eq!(request.header("referer"), Some(REFERER), "{target}");
        assert_eq!(
            request.header("user-agent"),
            Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36"
            ),
            "{target}"
        );
        assert_eq!(request.header("accept-encoding"), Some("identity"));
    }
}

impl Answer {
    pub(crate) fn tap_body(mut self, body: &[u8]) -> Self {
        self.body = body.to_vec();
        self
    }
}
