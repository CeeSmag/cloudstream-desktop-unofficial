//! Reads the WebKit cookie store for the challenged URL, with each cookie's attributes.

use cloudflare_bridge::{CookieParts, CookieValue};
use url::Url;
use webkit2gtk::{gio::Cancellable, CookieManager, CookieManagerExt};

use crate::browser_cookies::{checked, expiry_from_unix_seconds};
use crate::verification::Cookies;

pub(super) fn read_cookies(
    store: &CookieManager,
    target: &Url,
    done: impl FnOnce(Result<Cookies, String>) + 'static,
) {
    let scope = target.clone();
    store.cookies(target.as_str(), None::<&Cancellable>, move |result| {
        done(
            result
                .map(|cookies| filter_cookies(cookies, &scope))
                .map_err(|error| error.to_string()),
        );
    });
}

fn filter_cookies(cookies: Vec<soup::Cookie>, target: &Url) -> Cookies {
    cookies
        .into_iter()
        .filter_map(|mut cookie| {
            let parts = CookieParts {
                name: cookie.name()?.to_string(),
                value: CookieValue::new(cookie.value()?.to_string()),
                domain: cookie.domain()?.to_string(),
                path: cookie.path()?.to_string(),
                secure: cookie.is_secure(),
                http_only: cookie.is_http_only(),
                expires: expiry_from_unix_seconds(cookie.expires().map(|time| time.to_unix())),
            };
            checked(parts, target)
        })
        .collect()
}
