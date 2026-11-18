//! After a site's Cloudflare check passes, its cookies are copied from the check view into the app's own
//! browser (the page you look at). Without this, pictures from the site's image server (for example
//! `i.animepahe.pw`) are refused: Cloudflare protects that host too, and the pass cookie only lived in the
//! check view's separate profile. Names and domains are logged, never values.

use tauri::webview::cookie::Cookie;
use tauri::{AppHandle, Manager, Url, Webview};

/// Cookies worth copying to subdomains of the site (Cloudflare's own).
const CLOUDFLARE_PREFIX: &str = "cf_";

/// The cookies to set in the app's browser: each cookie as it is, plus, for Cloudflare's own cookies, a
/// copy for each of `extra_hosts` (the site's image server). The cookie type drops a leading dot of a
/// domain, so a copy cannot be made "for all subdomains"; one per named host is exact instead.
pub fn copies_of(cookies: &[Cookie<'static>], extra_hosts: &[String]) -> Vec<Cookie<'static>> {
    let mut copies = Vec::new();
    for cookie in cookies.iter().filter(|c| !c.value().is_empty()) {
        copies.push(cookie.clone());
        if !cookie.name().starts_with(CLOUDFLARE_PREFIX) {
            continue;
        }
        for host in extra_hosts
            .iter()
            .filter(|h| Some(h.as_str()) != cookie.domain())
        {
            let mut copy = cookie.clone();
            copy.set_domain(host.clone());
            copies.push(copy);
        }
    }
    copies
}
/// Copies the check view's cookies for `site` (and its image server) into the app's own browser; every
/// step and failure is logged. Returns how many cookies were set.
pub fn share_cookies(app: &AppHandle, from: &Webview, site: &Url) -> usize {
    let Some(main) = app.get_webview("main") else {
        tracing::error!("cannot share the check's cookies: the app's own browser view is missing");
        return 0;
    };
    let mut found = read(from, site);
    if let Some(host) = site.host_str() {
        if let Ok(images) = format!("https://i.{host}/").parse::<Url>() {
            found.extend(read(from, &images));
        }
    }
    let extra: Vec<String> = site
        .host_str()
        .map(|host| vec![format!("i.{host}")])
        .unwrap_or_default();
    let copies = copies_of(&found, &extra);
    let mut set = 0;
    for cookie in &copies {
        let what = format!("{}@{}", cookie.name(), cookie.domain().unwrap_or("-"));
        match main.set_cookie(cookie.clone()) {
            Ok(()) => {
                set += 1;
                tracing::info!(cookie = %what, secure = ?cookie.secure(), "copied a check cookie into the app's browser");
            }
            Err(error) => {
                tracing::warn!(cookie = %what, %error, "cannot copy a check cookie into the app's browser");
            }
        }
    }
    tracing::info!(
        site = site.host_str().unwrap_or("?"),
        found = found.len(),
        set,
        "shared the check's cookies with the app's browser"
    );
    set
}

fn read(view: &Webview, url: &Url) -> Vec<Cookie<'static>> {
    view.cookies_for_url(url.clone()).unwrap_or_else(|error| {
        tracing::warn!(%error, page = url.host_str().unwrap_or("?"), "cannot read the check's cookies to share them");
        Vec::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(name: &str, value: &str, domain: &str) -> Cookie<'static> {
        Cookie::build((name.to_owned(), value.to_owned()))
            .domain(domain.to_owned())
            .build()
    }

    fn hosts() -> Vec<String> {
        vec!["i.animepahe.pw".to_owned()]
    }

    #[test]
    fn a_cloudflare_cookie_is_also_set_for_the_image_server() {
        let copies = copies_of(&[cookie("cf_clearance", "x", "animepahe.pw")], &hosts());
        let domains: Vec<_> = copies.iter().map(|c| c.domain().unwrap_or("")).collect();
        assert_eq!(domains, vec!["animepahe.pw", "i.animepahe.pw"]);
    }

    #[test]
    fn a_cookie_already_for_the_image_server_is_copied_once() {
        let copies = copies_of(&[cookie("cf_clearance", "x", "i.animepahe.pw")], &hosts());
        assert_eq!(copies.len(), 1);
    }

    #[test]
    fn other_cookies_are_copied_as_they_are_and_empty_ones_not_at_all() {
        let copies = copies_of(
            &[
                cookie("session", "x", "animepahe.pw"),
                cookie("cf_x", "", "animepahe.pw"),
            ],
            &hosts(),
        );
        assert_eq!(copies.len(), 1);
        assert_eq!(copies.first().map(|c| c.name()), Some("session"));
    }
}
