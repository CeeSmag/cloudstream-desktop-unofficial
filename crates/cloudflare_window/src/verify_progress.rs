//! What to do next while a site's check is on screen.
//!
//! The `cf_clearance` cookie alone proves nothing (it can be left over from an earlier visit and no
//! longer be accepted), so once it is there and the browser's `User-Agent` is known, the site is
//! asked once from inside the window; only an answer without Cloudflare's challenge passes.

use std::time::{Duration, Instant};

/// How long the viewer gets to pass the check.
pub const VIEWER_TIME: Duration = Duration::from_secs(4 * 60);
/// The cookie that proves the check was passed.
pub const CLEARANCE_COOKIE: &str = "cf_clearance";

/// The state of one site's check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyProgress {
    /// When the check is given up on.
    pub deadline: Instant,
    /// A probe request runs in the window.
    pub probing: bool,
}

impl VerifyProgress {
    /// A check that starts `now`.
    pub fn starting(now: Instant) -> Self {
        Self {
            deadline: now + VIEWER_TIME,
            probing: false,
        }
    }
}

/// What the window shows right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    /// The window holds a `cf_clearance` cookie for the site.
    pub has_clearance: bool,
    /// The browser's `User-Agent` is known (Cloudflare ties the cookie to it).
    pub agent_known: bool,
    /// The page is on the site itself (a probe from another origin cannot reach the site).
    pub on_site: bool,
    /// False while the view is still on `about:blank` / empty (not a real off-site redirect).
    pub page_ready: bool,
}

/// The next step of a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Keep waiting for the viewer.
    Wait,
    /// Ask the site from inside the window whether the check is over.
    Probe,
    /// The page left the site after the check (some sites redirect elsewhere), so no probe can
    /// reach it: the clearance cookie is all there is, and it is handed over.
    PassOnCookie,
    /// Out of time.
    GiveUp,
}

/// The next step for `progress` given what the window shows at `now`.
pub fn next_step(progress: &VerifyProgress, seen: Observation, now: Instant) -> Step {
    if now >= progress.deadline {
        Step::GiveUp
    } else if !seen.page_ready || !(seen.has_clearance && seen.agent_known) || progress.probing {
        Step::Wait
    } else if seen.on_site {
        Step::Probe
    } else {
        Step::PassOnCookie
    }
}

/// True when `page` is a settled http(s) document (not the initial blank load).
pub fn page_is_ready(page: &str) -> bool {
    let trimmed = page.trim();
    !trimmed.is_empty()
        && trimmed != "about:blank"
        && (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
}

/// True when `page_host` is the site `host` or one of its subdomains.
pub fn is_on_site(page_host: &str, host: &str) -> bool {
    let page = page_host.to_ascii_lowercase();
    let site = host.to_ascii_lowercase();
    page == site
        || page
            .strip_suffix(&site)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

/// The host of a web address (`https://a.b/c` → `a.b`), empty when there is none.
pub fn host_of(url: &str) -> &str {
    let after_scheme = url.split_once("://").map_or("", |(_, rest)| rest);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let without_user = authority.rsplit('@').next().unwrap_or_default();
    without_user.split(':').next().unwrap_or_default()
}

/// True when a probe's answer shows the site no longer challenges the browser.
pub fn probe_passed(status: u16, headers: &[(String, String)]) -> bool {
    let challenged = headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("cf-mitigated") && value.contains("challenge")
    });
    !challenged && status != 403 && status != 503
}

/// True when `cookies` hold the clearance cookie with a value.
pub fn has_clearance(cookies: &[(String, String)]) -> bool {
    cookies
        .iter()
        .any(|(name, value)| name == CLEARANCE_COOKIE && !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(has_clearance: bool, agent_known: bool) -> Observation {
        Observation {
            has_clearance,
            agent_known,
            on_site: true,
            page_ready: true,
        }
    }

    #[test]
    fn next_step_page_off_site_passes_on_cookie() {
        let now = Instant::now();
        let progress = VerifyProgress::starting(now);
        let off_site = Observation {
            on_site: false,
            ..seen(true, true)
        };
        assert_eq!(next_step(&progress, off_site, now), Step::PassOnCookie);
        let no_cookie = Observation {
            on_site: false,
            ..seen(false, true)
        };
        assert_eq!(next_step(&progress, no_cookie, now), Step::Wait);
        let blank = Observation {
            on_site: false,
            page_ready: false,
            ..seen(true, true)
        };
        assert_eq!(next_step(&progress, blank, now), Step::Wait);
    }

    #[test]
    fn page_is_ready_rejects_blank() {
        assert!(!page_is_ready(""));
        assert!(!page_is_ready("about:blank"));
        assert!(page_is_ready("https://site.test/"));
    }

    #[test]
    fn is_on_site_and_host_of() {
        assert_eq!(
            host_of("https://User@Cdn.Site.test:8443/a?b#c"),
            "Cdn.Site.test"
        );
        assert_eq!(host_of("about:blank"), "");
        assert!(is_on_site("cdn.site.test", "site.test"));
        assert!(is_on_site("SITE.test", "site.test"));
        assert!(!is_on_site("evilsite.test", "site.test"));
    }

    #[test]
    fn next_step_waits_until_cookie_and_agent_then_probes_once() {
        let now = Instant::now();
        let mut progress = VerifyProgress::starting(now);
        assert_eq!(next_step(&progress, seen(false, true), now), Step::Wait);
        assert_eq!(next_step(&progress, seen(true, false), now), Step::Wait);
        assert_eq!(next_step(&progress, seen(true, true), now), Step::Probe);
        progress.probing = true;
        assert_eq!(next_step(&progress, seen(true, true), now), Step::Wait);
    }

    #[test]
    fn next_step_gives_up_after_the_viewer_time() {
        let now = Instant::now();
        let progress = VerifyProgress::starting(now);
        let late = now + VIEWER_TIME;
        assert_eq!(next_step(&progress, seen(true, true), late), Step::GiveUp);
    }

    #[test]
    fn probe_passed_rejects_challenge_answers() {
        assert!(probe_passed(200, &[]));
        assert!(!probe_passed(403, &[]));
        let challenged = [("CF-Mitigated".to_owned(), "challenge".to_owned())];
        assert!(!probe_passed(200, &challenged));
    }

    #[test]
    fn has_clearance_needs_a_value() {
        assert!(has_clearance(&[(
            "cf_clearance".to_owned(),
            "x".to_owned()
        )]));
        assert!(!has_clearance(&[(
            "cf_clearance".to_owned(),
            String::new()
        )]));
        assert!(!has_clearance(&[("__cf_bm".to_owned(), "x".to_owned())]));
    }
}
