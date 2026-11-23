//! A pass kept for later: what it is valid for, until when, and what it hands out for an address.
//!
//! A pass is governed by its `cf_clearance` cookie: the pass lives as long as that cookie (its `Expires`, set
//! by the site's "Challenge Passage"), and reaches exactly the hosts that cookie reaches (its `Domain`
//! attribute). So sharing a pass between `www.` and `api.` of a site happens only when the site itself set
//! its clearance for the whole domain, never because two host names look related.

use std::fmt;
use std::time::{Duration, SystemTime};

use url::Url;

use crate::allowed_hosts::is_multi_label_host;
use crate::pass_cookie::{CookieDomain, CookieExpiry, PassCookie, CLEARANCE_COOKIE};
use crate::pass_ids::{PassId, RunId};

/// A pass this close to its end is not handed out: the sidecar would use it for a request or two and be
/// challenged again.
pub const SERVE_MARGIN: Duration = Duration::from_secs(60);

/// No cookie outlives this (RFC 6265bis §5.5 caps `Expires` and `Max-Age` at 400 days, as browsers do), so
/// neither does a pass, whatever its expiry says.
pub const MAX_PASS_LIFETIME: Duration = Duration::from_secs(400 * 24 * 60 * 60);

/// The fields of a pass before they are checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassParts {
    /// Its id.
    pub id: PassId,
    /// The challenged address the check ran on.
    pub issued_for: Url,
    /// The browser identity the pass was issued to.
    pub user_agent: String,
    /// The browser's cookies for that address.
    pub cookies: Vec<PassCookie>,
    /// When the check was passed.
    pub obtained_at: SystemTime,
    /// The app run that saved it (its session cookies belong to that run).
    pub run: RunId,
}

/// Why a pass cannot be kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassRefusal {
    /// No user agent, or one with a line break (it would break the result file).
    BadAgent,
    /// No non-empty `cf_clearance` with an expiry that applies to the challenged address.
    NoClearance,
    /// The clearance's domain is a single label (`.com`): it would answer for unrelated sites.
    BroadDomain,
    /// The challenged address is not an http(s) page.
    NotAWebPage,
}

impl fmt::Display for PassRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BadAgent => "the pass has no usable user agent",
            Self::NoClearance => {
                "the pass holds no clearance with an expiry for the challenged address"
            }
            Self::BroadDomain => "the clearance is set for a whole top-level domain",
            Self::NotAWebPage => "the challenged address is not a web page",
        })
    }
}

impl std::error::Error for PassRefusal {}

/// A checked pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedPass {
    parts: PassParts,
    clearance: PassCookie,
    valid_until: SystemTime,
}

/// What a saved pass hands to the sidecar for one address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedPass {
    /// The pass, so the sidecar can name it if the site rejects it.
    pub id: PassId,
    /// The browser identity to send with it.
    pub user_agent: String,
    /// The cookies a browser would send to the address.
    pub cookies: Vec<PassCookie>,
    /// When its clearance ends.
    pub valid_until: SystemTime,
}

impl SavedPass {
    /// Checks `parts`. Cookies that do not apply to the challenged address, or whose domain is a single
    /// label, are dropped; the clearance must survive that.
    pub fn new(mut parts: PassParts) -> Result<Self, PassRefusal> {
        if !matches!(parts.issued_for.scheme(), "http" | "https") {
            return Err(PassRefusal::NotAWebPage);
        }
        if parts.user_agent.trim().is_empty() || parts.user_agent.chars().any(char::is_control) {
            return Err(PassRefusal::BadAgent);
        }
        let candidate = governing_clearance(&parts.cookies, &parts.issued_for).cloned();
        let clearance = candidate.ok_or(PassRefusal::NoClearance)?;
        if !is_multi_label_host(clearance.domain().host()) {
            return Err(PassRefusal::BroadDomain);
        }
        let expiry = expiry_time(&clearance).ok_or(PassRefusal::NoClearance)?;
        let valid_until = parts
            .obtained_at
            .checked_add(MAX_PASS_LIFETIME)
            .map_or(expiry, |cap| expiry.min(cap));
        let target = parts.issued_for.clone();
        parts.cookies.retain(|cookie| {
            cookie.applies_to(&target) && is_multi_label_host(cookie.domain().host())
        });
        Ok(Self {
            parts,
            clearance,
            valid_until,
        })
    }

    /// Its id.
    pub fn id(&self) -> &PassId {
        &self.parts.id
    }

    /// Everything it was built from.
    pub fn parts(&self) -> &PassParts {
        &self.parts
    }

    /// The hosts its clearance reaches.
    pub fn scope(&self) -> &CookieDomain {
        self.clearance.domain()
    }

    /// The governing `cf_clearance`.
    pub fn clearance(&self) -> &PassCookie {
        &self.clearance
    }

    /// When its clearance ends (at most [`MAX_PASS_LIFETIME`] after it was obtained).
    pub fn valid_until(&self) -> SystemTime {
        self.valid_until
    }

    /// True while it may still be handed out: more than [`SERVE_MARGIN`] before its clearance ends.
    pub fn is_fresh(&self, now: SystemTime) -> bool {
        now.checked_add(SERVE_MARGIN)
            .is_some_and(|soon| soon < self.valid_until)
    }

    /// True when a browser would send its clearance to `url`.
    pub fn covers(&self, url: &Url) -> bool {
        self.clearance.applies_to(url)
    }

    /// The cookies for `url` at `now`: those a browser would send, unexpired, and session cookies only
    /// within the run that saved them.
    pub fn served_for(&self, url: &Url, now: SystemTime, run: &RunId) -> ServedPass {
        let same_run = *run == self.parts.run;
        let cookies = self
            .parts
            .cookies
            .iter()
            .filter(|cookie| cookie.applies_to(url))
            .filter(|cookie| match cookie.expires() {
                CookieExpiry::Session => same_run,
                CookieExpiry::At(end) => end > now,
            })
            .cloned()
            .collect();
        ServedPass {
            id: self.parts.id.clone(),
            user_agent: self.parts.user_agent.clone(),
            cookies,
            valid_until: self.valid_until,
        }
    }
}

/// The clearance that decides the pass: non-empty, with an expiry, applying to `url`; the earliest one when
/// a browser holds several (the pass is no better than its weakest part).
fn governing_clearance<'a>(cookies: &'a [PassCookie], url: &Url) -> Option<&'a PassCookie> {
    cookies
        .iter()
        .filter(|cookie| cookie.name() == CLEARANCE_COOKIE && !cookie.value().is_empty())
        .filter(|cookie| cookie.applies_to(url))
        .filter_map(|cookie| expiry_time(cookie).map(|end| (end, cookie)))
        .min_by_key(|(end, _)| *end)
        .map(|(_, cookie)| cookie)
}

fn expiry_time(cookie: &PassCookie) -> Option<SystemTime> {
    match cookie.expires() {
        CookieExpiry::At(end) => Some(end),
        CookieExpiry::Session => None,
    }
}

#[cfg(test)]
#[path = "saved_pass_tests.rs"]
mod tests;
