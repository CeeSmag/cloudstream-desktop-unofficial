//! The addresses the player is given: `http://127.0.0.1:<port>/s/<token>/<id>.<extension>`.
//!
//! The address carries no part of the real URL (a signed link would end up in the player's log);
//! `<id>` is a number the session maps back to it.

/// The first path segment of every proxied address.
const ROUTE: &str = "s";

/// Where a request points inside the proxy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProxiedPath {
    pub(crate) token: String,
    pub(crate) id: usize,
}

/// The address the player opens for entry `id` of the session `token`.
pub(crate) fn proxied_url(port: u16, token: &str, id: usize, extension: &str) -> String {
    format!("http://127.0.0.1:{port}/{ROUTE}/{token}/{id}.{extension}")
}

/// Reads the path of a request (a query string is ignored); `None` for anything else.
pub(crate) fn parse_path(target: &str) -> Option<ProxiedPath> {
    let path = target.split('?').next()?;
    let mut parts = path.strip_prefix('/')?.split('/');
    if parts.next()? != ROUTE {
        return None;
    }
    let token = parts.next().filter(|token| is_token(token))?;
    let (id, extension) = parts.next()?.split_once('.')?;
    let id = id.parse().ok()?;
    if parts.next().is_some() || !extension.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(ProxiedPath {
        token: token.to_owned(),
        id,
    })
}

fn is_token(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_path_reads_back_what_proxied_url_wrote() {
        let url = proxied_url(5000, "abc123", 42, "ts");
        assert_eq!(url, "http://127.0.0.1:5000/s/abc123/42.ts");
        let path = url
            .strip_prefix("http://127.0.0.1:5000")
            .expect("the prefix");
        assert_eq!(
            parse_path(path),
            Some(ProxiedPath {
                token: "abc123".to_owned(),
                id: 42
            })
        );
    }

    #[test]
    fn parse_path_ignores_a_query_string() {
        assert_eq!(parse_path("/s/ab/7.m3u8?x=1").map(|path| path.id), Some(7));
    }

    #[test]
    fn parse_path_rejects_other_routes_and_malformed_paths() {
        for target in [
            "/",
            "/s",
            "/s/ab",
            "/s/ab/",
            "/s/ab/x.ts",
            "/s/ab/1",
            "/s/ab/1.ts/more",
            "/s//1.ts",
            "/s/zz/1.ts",
            "/other/ab/1.ts",
            "s/ab/1.ts",
            "/s/ab/-1.ts",
            "/s/ab/1.t s",
            "/s/ab/../1.ts",
        ] {
            assert_eq!(parse_path(target), None, "{target:?}");
        }
    }

    #[test]
    fn parse_path_rejects_an_id_too_big_for_a_number() {
        assert_eq!(parse_path("/s/ab/99999999999999999999999999.ts"), None);
    }
}
