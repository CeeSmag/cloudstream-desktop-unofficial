//! Reads the WebKit cookie store for the challenged URL.

use url::Url;
use webkit2gtk::{gio::Cancellable, CookieManager, CookieManagerExt};

use crate::verification::{cookie_matches, Cookies};

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
            if !cookie_matches(
                target,
                &cookie.domain()?,
                &cookie.path()?,
                cookie.is_secure(),
            ) {
                return None;
            }
            Some((cookie.name()?.to_string(), cookie.value()?.to_string()))
        })
        .collect()
}
