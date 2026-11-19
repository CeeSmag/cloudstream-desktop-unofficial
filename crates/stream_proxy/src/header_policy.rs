//! Which headers the proxy sends to the stream's hosts: the provider's own, on every request.

use crate::stream_spec::StreamSpec;
use crate::url_parts::host_of;

/// Same Chrome desktop string as `core_model::DEFAULT_STREAM_USER_AGENT` / CloudStream Android
/// (`MainAPI.USER_AGENT`). Kept as a literal so this crate stays free of internal deps; a host that
/// accepts the player must accept the proxy with the same agent.
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";

/// Headers the proxy sets itself or that only make sense on the player's own connection.
const NEVER_FORWARDED: &[&str] = &[
    "host",
    "content-length",
    "connection",
    "transfer-encoding",
    "keep-alive",
    "te",
    "trailer",
    "upgrade",
    "expect",
    "proxy-authorization",
    "proxy-connection",
    "accept-encoding",
    "range",
    "if-range",
];

/// Headers that identify the viewer to one site and must not follow a link to another one.
const SAME_SITE_ONLY: &[&str] = &["cookie", "authorization"];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// True for a header that can be sent as it is: a plain name and a value without line breaks.
pub(crate) fn is_valid_header(name: &str, value: &str) -> bool {
    !name.is_empty() && name.chars().all(is_token_char) && !value.contains(['\r', '\n', '\0'])
}

fn is_named(name: &str, list: &[&str]) -> bool {
    list.iter().any(|listed| name.eq_ignore_ascii_case(listed))
}

fn has_header(headers: &[(String, String)], wanted: &str) -> bool {
    headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(wanted))
}

/// The provider's headers without the ones that cannot be sent or that the proxy sets itself.
fn sendable_headers(provided: &[(String, String)]) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    for (name, value) in provided {
        if !is_valid_header(name, value) {
            tracing::warn!(header = %name, "a stream header cannot be sent and was left out");
        } else if !is_named(name, NEVER_FORWARDED) {
            headers.push((name.clone(), value.clone()));
        }
    }
    headers
}

/// The headers for the requests one stream makes.
#[derive(Debug, Clone)]
pub(crate) struct HeaderPolicy {
    headers: Vec<(String, String)>,
    home_host: Option<String>,
}

impl HeaderPolicy {
    /// Builds the policy of a stream: its headers cleaned up, its referer added when not already
    /// one of them, and the user agent the player would have sent.
    pub(crate) fn new(spec: &StreamSpec) -> Self {
        let mut headers = sendable_headers(&spec.headers);
        if !has_header(&headers, "referer")
            && !spec.referer.is_empty()
            && is_valid_header("Referer", &spec.referer)
        {
            headers.push(("Referer".to_owned(), spec.referer.clone()));
        }
        if !has_header(&headers, "user-agent") {
            headers.push(("User-Agent".to_owned(), DEFAULT_USER_AGENT.to_owned()));
        }
        Self {
            headers,
            home_host: host_of(&spec.url),
        }
    }

    /// The headers to send to `url`. Cookies and credentials go only to the host the stream
    /// started on; a redirect or a playlist that names another host gets the rest.
    pub(crate) fn for_url(&self, url: &str) -> Vec<(String, String)> {
        let same_site = host_of(url) == self.home_host;
        let mut headers: Vec<(String, String)> = self
            .headers
            .iter()
            .filter(|(name, _)| same_site || !is_named(name, SAME_SITE_ONLY))
            .cloned()
            .collect();
        headers.push(("Accept-Encoding".to_owned(), "identity".to_owned()));
        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(headers: &[(&str, &str)], referer: &str) -> StreamSpec {
        StreamSpec {
            url: "https://home.test/master.m3u8".to_owned(),
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            referer: referer.to_owned(),
        }
    }

    fn value_of(headers: &[(String, String)], wanted: &str) -> Option<String> {
        headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value.clone())
    }

    #[test]
    fn is_valid_header_accepts_plain_headers_and_rejects_injection() {
        assert!(is_valid_header("X-Token", "abc def"));
        assert!(!is_valid_header("", "x"));
        assert!(!is_valid_header("Bad Name", "x"));
        assert!(!is_valid_header("X", "a\r\nInjected: 1"));
        assert!(!is_valid_header("X", "nul\0"));
        assert!(!is_valid_header("X:", "x"));
    }

    #[test]
    fn for_url_sends_the_providers_headers_and_the_referer() {
        let policy = HeaderPolicy::new(&spec(&[("X-Token", "t")], "https://site.test/"));
        let sent = policy.for_url("https://home.test/seg.ts");
        assert_eq!(value_of(&sent, "x-token").as_deref(), Some("t"));
        assert_eq!(
            value_of(&sent, "referer").as_deref(),
            Some("https://site.test/")
        );
    }

    #[test]
    fn new_keeps_an_explicit_referer_header_and_adds_no_second_one() {
        let policy = HeaderPolicy::new(&spec(
            &[("referer", "https://own.test/")],
            "https://site.test/",
        ));
        let sent = policy.for_url("https://home.test/seg.ts");
        let referers: Vec<_> = sent
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case("referer"))
            .collect();
        assert_eq!(referers.len(), 1);
        assert_eq!(
            value_of(&sent, "referer").as_deref(),
            Some("https://own.test/")
        );
    }

    #[test]
    fn for_url_uses_the_players_user_agent_unless_the_stream_names_one() {
        let sent = HeaderPolicy::new(&spec(&[], "")).for_url("https://home.test/a");
        assert_eq!(
            value_of(&sent, "user-agent").as_deref(),
            Some(DEFAULT_USER_AGENT)
        );
        let own = HeaderPolicy::new(&spec(&[("User-Agent", "Custom/1")], ""));
        assert_eq!(
            value_of(&own.for_url("https://home.test/a"), "user-agent").as_deref(),
            Some("Custom/1")
        );
    }

    #[test]
    fn for_url_never_forwards_range_host_or_encoding_from_the_provider() {
        let headers = [
            ("Range", "bytes=0-1"),
            ("Host", "evil.test"),
            ("Accept-Encoding", "gzip"),
            ("Connection", "keep-alive"),
        ];
        let sent = HeaderPolicy::new(&spec(&headers, "")).for_url("https://home.test/a");
        assert_eq!(value_of(&sent, "range"), None);
        assert_eq!(value_of(&sent, "host"), None);
        assert_eq!(value_of(&sent, "connection"), None);
        assert_eq!(
            value_of(&sent, "accept-encoding").as_deref(),
            Some("identity")
        );
    }

    #[test]
    fn for_url_keeps_cookies_on_the_home_host_only() {
        let policy = HeaderPolicy::new(&spec(
            &[("Cookie", "a=1"), ("Authorization", "Bearer x")],
            "",
        ));
        let home = policy.for_url("https://HOME.test:8443/seg.ts");
        assert_eq!(value_of(&home, "cookie").as_deref(), Some("a=1"));
        assert_eq!(
            value_of(&home, "authorization").as_deref(),
            Some("Bearer x")
        );
        let other = policy.for_url("https://cdn.other.test/seg.ts");
        assert_eq!(value_of(&other, "cookie"), None);
        assert_eq!(value_of(&other, "authorization"), None);
        assert_eq!(value_of(&other, "referer"), None, "no referer was given");
    }

    #[test]
    fn new_leaves_out_a_header_that_could_inject_another() {
        let policy = HeaderPolicy::new(&spec(&[("X-Ok", "1"), ("X-Bad", "a\r\nX-Evil: 1")], ""));
        let sent = policy.for_url("https://home.test/a");
        assert_eq!(value_of(&sent, "x-ok").as_deref(), Some("1"));
        assert_eq!(value_of(&sent, "x-bad"), None);
        assert_eq!(value_of(&sent, "x-evil"), None);
    }

    #[test]
    fn the_referer_goes_to_other_hosts_too() {
        let policy = HeaderPolicy::new(&spec(&[], "https://site.test/"));
        let other = policy.for_url("https://cdn.other.test/seg.ts");
        assert_eq!(
            value_of(&other, "referer").as_deref(),
            Some("https://site.test/")
        );
    }
}
