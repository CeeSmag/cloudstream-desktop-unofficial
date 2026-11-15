//! Takes a URL apart far enough to resolve references and compare hosts. No `url` crate: the
//! proxy only needs the five pieces below and must not guess at anything else.

/// A URL split into its pieces; the fragment is dropped, nothing is decoded.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UrlParts<'a> {
    pub(crate) scheme: &'a str,
    pub(crate) authority: &'a str,
    pub(crate) path: &'a str,
    pub(crate) query: Option<&'a str>,
}

fn is_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Splits `scheme://authority/path?query`; `None` when there is no scheme and authority.
pub(crate) fn split_url(url: &str) -> Option<UrlParts<'_>> {
    let without_fragment = url.split('#').next()?;
    let (scheme, rest) = without_fragment.split_once("://")?;
    if !is_scheme(scheme) {
        return None;
    }
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = rest.get(..end)?;
    let tail = rest.get(end..)?;
    let (path, query) = match tail.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (tail, None),
    };
    Some(UrlParts {
        scheme,
        authority,
        path,
        query,
    })
}

/// True for a reference that starts with its own scheme (`https:`, `data:`, `mailto:`).
pub(crate) fn has_scheme(reference: &str) -> bool {
    reference
        .split_once(':')
        .is_some_and(|(scheme, _)| is_scheme(scheme))
}

/// The host of `url` in lower case, without user name or port; `None` for a URL without one.
pub(crate) fn host_of(url: &str) -> Option<String> {
    let parts = split_url(url)?;
    let host_and_port = parts.authority.rsplit('@').next()?;
    let host = if host_and_port.starts_with('[') {
        let end = host_and_port.find(']')?;
        host_and_port.get(..=end)?
    } else {
        host_and_port.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// True when `url` is an `http` or `https` address, the only kinds the proxy fetches.
pub(crate) fn is_http(url: &str) -> bool {
    split_url(url).is_some_and(|parts| {
        parts.scheme.eq_ignore_ascii_case("http") || parts.scheme.eq_ignore_ascii_case("https")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_url_full_address_returns_every_piece() {
        let parts = split_url("https://user@cdn.test:8443/a/b.m3u8?token=1&x=2#frag");
        assert_eq!(
            parts,
            Some(UrlParts {
                scheme: "https",
                authority: "user@cdn.test:8443",
                path: "/a/b.m3u8",
                query: Some("token=1&x=2"),
            })
        );
    }

    #[test]
    fn split_url_without_path_returns_an_empty_path() {
        let parts = split_url("http://host.test?x=1").expect("a URL");
        assert_eq!(
            (parts.authority, parts.path, parts.query),
            ("host.test", "", Some("x=1"))
        );
        let parts = split_url("http://host.test").expect("a URL");
        assert_eq!((parts.path, parts.query), ("", None));
    }

    #[test]
    fn split_url_without_scheme_or_authority_returns_none() {
        assert_eq!(split_url("/just/a/path"), None);
        assert_eq!(split_url("segment.ts"), None);
        assert_eq!(split_url("://nohost"), None);
        assert_eq!(split_url("1http://host"), None);
    }

    #[test]
    fn host_of_ignores_user_port_and_case() {
        assert_eq!(
            host_of("https://User:pw@CDN.Test:8443/x").as_deref(),
            Some("cdn.test")
        );
        assert_eq!(host_of("http://[::1]:80/x").as_deref(), Some("[::1]"));
        assert_eq!(host_of("http:///x"), None);
        assert_eq!(host_of("no-url"), None);
    }

    #[test]
    fn has_scheme_tells_absolute_references_from_relative_ones() {
        assert!(has_scheme("https://a/b"));
        assert!(has_scheme("data:text/plain,x"));
        assert!(!has_scheme("seg/1.ts"));
        assert!(!has_scheme("./a:b.ts"));
        assert!(!has_scheme("/a:b"));
    }

    #[test]
    fn is_http_accepts_only_web_addresses() {
        assert!(is_http("HTTP://a/b"));
        assert!(is_http("https://a/b"));
        assert!(!is_http("file:///etc/passwd"));
        assert!(!is_http("ftp://a/b"));
        assert!(!is_http("data:x"));
    }
}
