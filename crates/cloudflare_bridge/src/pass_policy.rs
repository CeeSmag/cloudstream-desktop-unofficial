//! Whether a verification request is answered from a saved pass, and with which one. Pure: the watcher
//! gathers the facts (the request, the live passes, the viewer's permissions, the guard, the time) and acts
//! on the decision.

use std::time::SystemTime;

use url::Url;

use crate::pass_ids::RunId;
use crate::reask_guard::ReaskGuard;
use crate::saved_pass::{SavedPass, ServedPass};
use crate::{AllowedHosts, RejectedPass, RequestNeed, VerifyRequest};

/// Why a request goes to the check window instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BypassReason {
    /// A viewer's retry: they asked for a fresh check.
    Standalone,
    /// The sidecar reaches this origin only through the browser window.
    NeedsBrowser,
    /// The sidecar reports a saved pass refused.
    RejectedPass,
    /// The request's address is not a web page.
    NotAWebPage,
    /// No live pass reaches this address.
    NoPass,
    /// A pass reaches it, but its site is no longer allowed to be verified.
    NotAllowed,
    /// Every pass that reaches it was just handed to this origin (see `reask_guard`).
    Guarded,
}

impl BypassReason {
    /// For the log.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Standalone => "a viewer's retry",
            Self::NeedsBrowser => "the origin needs the browser window",
            Self::RejectedPass => "the site refused a saved pass",
            Self::NotAWebPage => "not a web page",
            Self::NoPass => "no saved pass",
            Self::NotAllowed => "the pass's site is not allowed",
            Self::Guarded => "the pass was just handed to this origin",
        }
    }
}

/// What to do with a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PassDecision {
    /// Answer it at once with this pass.
    Serve(ServedPass),
    /// Route it as before (permission, check window).
    Bypass(BypassReason),
}

/// The facts a decision is made from.
pub(crate) struct PassQuery<'a> {
    pub(crate) request: &'a VerifyRequest,
    pub(crate) origin: &'a str,
    pub(crate) passes: &'a [SavedPass],
    pub(crate) allowed: &'a AllowedHosts,
    pub(crate) guard: &'a ReaskGuard,
    pub(crate) now: SystemTime,
    pub(crate) run: &'a RunId,
}

/// Decides how `query.request` is answered.
pub(crate) fn decide(query: &PassQuery<'_>) -> PassDecision {
    let request = query.request;
    if request.standalone {
        return PassDecision::Bypass(BypassReason::Standalone);
    }
    if request.reason.need == RequestNeed::Browser {
        return PassDecision::Bypass(BypassReason::NeedsBrowser);
    }
    if request.reason.rejected != RejectedPass::None {
        return PassDecision::Bypass(BypassReason::RejectedPass);
    }
    let Ok(url) = Url::parse(&request.url) else {
        return PassDecision::Bypass(BypassReason::NotAWebPage);
    };
    let reaching: Vec<&SavedPass> = query
        .passes
        .iter()
        .filter(|pass| pass.is_fresh(query.now) && pass.covers(&url))
        .collect();
    if reaching.is_empty() {
        return PassDecision::Bypass(BypassReason::NoPass);
    }
    let allowed: Vec<&SavedPass> = reaching
        .into_iter()
        .filter(|pass| is_issuer_allowed(pass, query.allowed))
        .collect();
    if allowed.is_empty() {
        return PassDecision::Bypass(BypassReason::NotAllowed);
    }
    allowed
        .into_iter()
        .filter(|pass| !query.guard.blocks(query.origin, pass.id(), query.now))
        .max_by_key(|pass| pass.parts().obtained_at)
        .map_or(PassDecision::Bypass(BypassReason::Guarded), |pass| {
            PassDecision::Serve(pass.served_for(&url, query.now, query.run))
        })
}

/// The viewer's consent to verify a site covers using what that verification produced.
fn is_issuer_allowed(pass: &SavedPass, allowed: &AllowedHosts) -> bool {
    pass.parts()
        .issued_for
        .host_str()
        .is_some_and(|host| allowed.is_allowed(host))
}

#[cfg(test)]
#[path = "pass_policy_tests.rs"]
mod tests;
