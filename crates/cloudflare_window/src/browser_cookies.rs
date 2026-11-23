//! What both browsers' cookie readers share: the check that a cookie may be handed on and the
//! conversion of each engine's expiry into [`CookieExpiry`]. Pure, so it is tested on every OS.

use std::time::{Duration, UNIX_EPOCH};

use cloudflare_bridge::{CookieExpiry, CookieParts, PassCookie};
use url::Url;

/// A cookie the browser would send to `target`. One that cannot be handed on (a name or value that would
/// break the result file) is left out and logged by name only.
pub(crate) fn checked(parts: CookieParts, target: &Url) -> Option<PassCookie> {
    let name = parts.name.clone();
    match PassCookie::new(parts) {
        Ok(cookie) => cookie.applies_to(target).then_some(cookie),
        Err(refusal) => {
            tracing::debug!(cookie = %name, %refusal, "a browser cookie cannot be handed on");
            None
        }
    }
}

/// WebKit's `expires` as whole seconds since 1970; None (or a time before 1970) is a session cookie.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn expiry_from_unix_seconds(seconds: Option<i64>) -> CookieExpiry {
    seconds
        .and_then(|secs| u64::try_from(secs).ok())
        .map(Duration::from_secs)
        .map_or(CookieExpiry::Session, since_1970)
}

/// WebView2's `expires` as fractional seconds since 1970; None, a negative or a non-finite number is a
/// session cookie (WebView2 reports `-1` for those).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn expiry_from_fractional_seconds(seconds: Option<f64>) -> CookieExpiry {
    seconds
        .filter(|secs| secs.is_finite() && *secs >= 0.0)
        .and_then(|secs| Duration::try_from_secs_f64(secs).ok())
        .map_or(CookieExpiry::Session, since_1970)
}

/// A time this far after 1970; one the platform's clock cannot hold is no usable expiry.
fn since_1970(since: Duration) -> CookieExpiry {
    UNIX_EPOCH
        .checked_add(since)
        .map_or(CookieExpiry::Session, CookieExpiry::At)
}

#[cfg(test)]
mod tests {
    use cloudflare_bridge::CookieValue;

    use super::*;

    fn parts(name: &str, domain: &str) -> CookieParts {
        CookieParts {
            name: name.into(),
            value: CookieValue::new("v"),
            domain: domain.into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            expires: CookieExpiry::Session,
        }
    }

    #[test]
    fn checked_keeps_only_valid_cookies_the_browser_sends_to_the_target() {
        let target = Url::parse("https://www.site.test/watch").expect("url");
        assert!(checked(parts("cf_clearance", ".site.test"), &target).is_some());
        assert!(checked(parts("cf_clearance", "other.test"), &target).is_none());
        assert!(checked(parts("bad name", ".site.test"), &target).is_none());
        let plain = Url::parse("http://www.site.test/watch").expect("url");
        assert!(checked(parts("cf_clearance", ".site.test"), &plain).is_none());
    }

    #[test]
    fn expiry_from_unix_seconds_reads_whole_seconds_and_session_cookies() {
        assert_eq!(expiry_from_unix_seconds(None), CookieExpiry::Session);
        assert_eq!(expiry_from_unix_seconds(Some(-5)), CookieExpiry::Session);
        assert_eq!(
            expiry_from_unix_seconds(Some(0)),
            CookieExpiry::At(UNIX_EPOCH)
        );
        // The largest second count never panics, whatever range the platform's clock has.
        let _ = expiry_from_unix_seconds(Some(i64::MAX));
        assert_eq!(
            expiry_from_unix_seconds(Some(1_700_000_000)),
            CookieExpiry::At(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        );
    }

    #[test]
    fn since_1970_refuses_a_time_the_clock_cannot_hold() {
        assert_eq!(since_1970(Duration::MAX), CookieExpiry::Session);
        assert_eq!(
            since_1970(Duration::from_secs(5)),
            CookieExpiry::At(UNIX_EPOCH + Duration::from_secs(5))
        );
    }

    #[test]
    fn expiry_from_fractional_seconds_reads_fractions_and_rejects_nonsense() {
        assert_eq!(expiry_from_fractional_seconds(None), CookieExpiry::Session);
        for nonsense in [
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            1e300,
        ] {
            assert_eq!(
                expiry_from_fractional_seconds(Some(nonsense)),
                CookieExpiry::Session,
                "{nonsense}"
            );
        }
        assert_eq!(
            expiry_from_fractional_seconds(Some(0.0)),
            CookieExpiry::At(UNIX_EPOCH)
        );
        assert_eq!(
            expiry_from_fractional_seconds(Some(10.5)),
            CookieExpiry::At(UNIX_EPOCH + Duration::from_millis(10_500))
        );
    }
}
