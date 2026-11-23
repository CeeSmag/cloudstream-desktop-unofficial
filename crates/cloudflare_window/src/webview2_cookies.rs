//! Reads the WebView2 cookie store for the challenged URL, with each cookie's attributes.

use cloudflare_bridge::{CookieParts, CookieValue};
use url::Url;
use windows_webview::{Cookie, WebViewHost};

use crate::browser_cookies::{checked, expiry_from_fractional_seconds};
use crate::verification::Cookies;

pub(super) fn read_cookies(
    native: &WebViewHost,
    target: &Url,
    done: impl FnOnce(windows_webview::Result<Cookies>) + 'static,
) -> windows_webview::Result<()> {
    let scope = target.clone();
    native
        .webview()
        .cookie_manager()?
        .get_cookies(target.as_str(), move |result| {
            done(result.map(|cookies| filter_cookies(cookies, &scope)));
        })
}

fn filter_cookies(cookies: Vec<Cookie>, target: &Url) -> Cookies {
    cookies
        .into_iter()
        .filter_map(|cookie| checked(webview2_parts(cookie), target))
        .collect()
}

/// WebView2's cookie as the fields the bridge checks (a leading dot marks a domain cookie, as in WebKit).
fn webview2_parts(cookie: Cookie) -> CookieParts {
    CookieParts {
        name: cookie.name,
        value: CookieValue::new(cookie.value),
        domain: cookie.domain,
        path: cookie.path,
        secure: cookie.is_secure,
        http_only: cookie.is_http_only,
        expires: expiry_from_fractional_seconds(cookie.expires),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use cloudflare_bridge::{CookieDomain, CookieExpiry};
    use windows_webview::SameSite;

    use super::*;

    fn raw_cookie(name: &str, domain: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "token-1".into(),
            domain: domain.into(),
            path: "/".into(),
            is_secure: true,
            is_http_only: true,
            same_site: SameSite::Lax,
            expires: Some(1_800_000_000.5),
        }
    }

    #[test]
    fn webview2_parts_preserves_domain_cookie_attributes() {
        let domain_parts = webview2_parts(raw_cookie("cf_clearance", ".site.test"));
        assert_eq!(domain_parts.name, "cf_clearance");
        assert_eq!(domain_parts.value.expose(), "token-1");
        assert_eq!(domain_parts.domain, ".site.test");
        assert_eq!(domain_parts.path, "/");
        assert!(domain_parts.secure && domain_parts.http_only);
        assert_eq!(
            domain_parts.expires,
            CookieExpiry::At(UNIX_EPOCH + Duration::from_millis(1_800_000_000_500))
        );
    }

    #[test]
    fn webview2_parts_preserves_host_only_session_cookie() {
        let mut session = raw_cookie("sess", "www.site.test");
        session.is_secure = false;
        session.is_http_only = false;
        session.expires = None;
        let session_parts = webview2_parts(session);
        assert_eq!(session_parts.domain, "www.site.test");
        assert!(!session_parts.secure && !session_parts.http_only);
        assert_eq!(session_parts.expires, CookieExpiry::Session);
    }

    #[test]
    fn filter_cookies_keeps_only_valid_applying_domain_and_host_only_cookies() {
        let target = Url::parse("https://www.site.test/watch/1").expect("url");
        let domain = raw_cookie("cf_clearance", ".site.test");
        let host_only = raw_cookie("host_id", "www.site.test");
        let sibling_host_only = raw_cookie("other_sub", "api.site.test");
        let foreign = raw_cookie("foreign", ".other.test");
        let unsafe_name = raw_cookie("bad name", ".site.test");
        let mut wrong_path = raw_cookie("scoped", ".site.test");
        wrong_path.path = "/admin".into();

        let kept = filter_cookies(
            vec![
                domain,
                host_only,
                sibling_host_only,
                foreign,
                unsafe_name,
                wrong_path,
            ],
            &target,
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].name(), "cf_clearance");
        assert_eq!(kept[0].domain(), &CookieDomain::Domain("site.test".into()));
        assert_eq!(kept[1].name(), "host_id");
        assert_eq!(
            kept[1].domain(),
            &CookieDomain::HostOnly("www.site.test".into())
        );

        let plain_http = Url::parse("http://www.site.test/watch/1").expect("url");
        assert!(
            filter_cookies(vec![raw_cookie("cf_clearance", ".site.test")], &plain_http).is_empty()
        );
    }
}
