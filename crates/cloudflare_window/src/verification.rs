//! Evidence from the browser's main navigation and the verification lifecycle.

use cloudflare_bridge::{PassCookie, CLEARANCE_COOKIE};
use url::Url;

/// The browser's cookies for the challenged address, with their attributes.
pub(crate) type Cookies = Vec<PassCookie>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentId(pub(crate) u64);

#[derive(Debug)]
pub(crate) struct MainResponse {
    pub(crate) url: Url,
    pub(crate) status: u16,
    pub(crate) challenge: bool,
    pub(crate) html: bool,
    pub(crate) attachment: bool,
    pub(crate) redirect: Option<Url>,
}

impl MainResponse {
    pub(crate) fn from_headers(
        url: Url,
        status: u16,
        header: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let redirect = (300..400)
            .contains(&status)
            .then(|| url.join(&header("location")?).ok())
            .flatten();
        Self {
            url,
            status,
            redirect,
            challenge: header("cf-mitigated")
                .as_deref()
                .is_some_and(is_challenge_header),
            html: header("content-type").as_deref().is_some_and(is_html_type),
            attachment: header("content-disposition")
                .as_deref()
                .is_some_and(is_attachment),
        }
    }

    pub(crate) fn is_interstitial(&self) -> bool {
        self.challenge && self.html && !self.attachment
    }

    /// The site itself answered, whatever it said: Cloudflare's gate is behind the browser. A 404 or a
    /// 500 is the site's own answer. 403, 429 and 503 can still be the gate's (a block or a rate limit),
    /// which no cookie opens, so they are not evidence of a passage.
    fn answered(&self) -> bool {
        !self.challenge && self.status >= 200 && !matches!(self.status, 403 | 429 | 503)
    }

    pub(crate) fn leaves_origin(&self, target: &Url) -> bool {
        self.url.origin() != target.origin()
            || self
                .redirect
                .as_ref()
                .is_some_and(|url| url.origin() != target.origin())
    }
}

#[derive(Debug)]
pub(crate) struct BrowserIdentity {
    pub(crate) agent: String,
    pub(crate) cookies: Cookies,
}

#[derive(Debug)]
pub(crate) struct ObservedResponse {
    pub(crate) document: DocumentId,
    pub(crate) response: MainResponse,
    pub(crate) cookies: Cookies,
}

#[derive(Debug)]
pub(crate) enum BrowserEvent {
    Ready,
    Started(DocumentId),
    Committed(DocumentId, Url),
    Response(ObservedResponse),
    Identity(Result<BrowserIdentity, String>),
    Unavailable(String),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Wait,
    /// A challenge was passed: the cookies (and browser identity) are the credentials to hand over.
    Passed(Cookies),
    /// The browser was never challenged: only the sidecar's own connection is. There is nothing to hand
    /// over, so the sidecar must make this origin's requests through the browser document.
    BrowserClear,
    /// The browser was never challenged and the target origin redirected this request to another origin
    /// (for example a permanent domain move). The browser aborts the off-origin load and returns the
    /// redirect so the sidecar's guarded HTTP client can validate and follow it.
    Redirect {
        status: u16,
        location: Url,
    },
    Unconfirmed,
}

pub(crate) struct Verification {
    target: Url,
    baseline: Cookies,
    current: Option<DocumentId>,
    challenge: Option<DocumentId>,
    committed: Option<DocumentId>,
    encountered_challenge: bool,
}

impl Verification {
    pub(crate) fn new(target: Url, baseline: Cookies) -> Self {
        Self {
            target,
            baseline,
            current: None,
            challenge: None,
            committed: None,
            encountered_challenge: false,
        }
    }

    pub(crate) fn start(&mut self, document: DocumentId) {
        self.current = Some(document);
        self.challenge = None;
        self.committed = None;
    }

    pub(crate) fn commit(&mut self, document: DocumentId) {
        if self.current == Some(document) {
            self.committed = Some(document);
        }
    }

    pub(crate) fn visible_document(&self) -> Option<DocumentId> {
        self.challenge.filter(|id| self.committed == Some(*id))
    }

    pub(crate) fn response(
        &mut self,
        document: DocumentId,
        response: &MainResponse,
        cookies: Cookies,
    ) -> Decision {
        if self.current != Some(document) {
            return Decision::Wait;
        }
        self.challenge = None;
        if self.target.origin() != response.url.origin() {
            return Decision::Unconfirmed;
        }
        if response.is_interstitial() && matches_target(&self.target, &response.url) {
            self.encountered_challenge = true;
            self.challenge = Some(document);
            return Decision::Wait;
        }
        if self.is_passage(response, &cookies) {
            return Decision::Passed(cookies);
        }
        if self.is_browser_clear(response) {
            return Decision::BrowserClear;
        }
        if let Some((status, location)) = self.browser_redirect(response) {
            return Decision::Redirect { status, location };
        }
        self.unsettled(response)
    }

    /// Neither passed nor clear: a redirect or a challenge page is still in progress, anything else is
    /// not evidence of a passage.
    fn unsettled(&self, response: &MainResponse) -> Decision {
        if response.leaves_origin(&self.target)
            || (!response.challenge && !(300..400).contains(&response.status))
        {
            Decision::Unconfirmed
        } else {
            Decision::Wait
        }
    }

    /// The browser went straight through (no challenge was ever shown to it) and the approved origin
    /// answered with an HTTP redirect to another HTTP(S) origin. A redirect after a challenge without a
    /// fresh clearance cookie is *never* this: it stays unconfirmed.
    fn browser_redirect(&self, response: &MainResponse) -> Option<(u16, Url)> {
        if self.encountered_challenge
            || response.challenge
            || response.attachment
            || !matches!(response.status, 301 | 302 | 303 | 307 | 308)
            || response.url.origin() != self.target.origin()
        {
            return None;
        }
        let location = response.redirect.as_ref()?;
        let valid_target = matches!(location.scheme(), "http" | "https")
            && location.username().is_empty()
            && location.password().is_none()
            && location.origin() != self.target.origin();
        valid_target.then(|| (response.status, location.clone()))
    }

    /// The browser went straight through (no challenge was ever shown to it) and landed on a same-origin
    /// document. A challenge followed by a missing or stale cookie is *not* this: that stays unconfirmed.
    fn is_browser_clear(&self, response: &MainResponse) -> bool {
        !self.encountered_challenge
            && !response.challenge
            && !response.attachment
            && response.answered()
            && !(300..400).contains(&response.status)
            && response.url.origin() == self.target.origin()
    }

    fn is_passage(&self, response: &MainResponse, cookies: &Cookies) -> bool {
        response.answered()
            && matches_target(&self.target, &response.url)
            && cookies.iter().any(|cookie| {
                cookie.name() == CLEARANCE_COOKIE
                    && !cookie.value().is_empty()
                    && (!self.encountered_challenge
                        || !self.baseline.iter().any(|old| old.same_credential(cookie)))
            })
    }
}

pub(crate) fn matches_target(target: &Url, candidate: &Url) -> bool {
    target.origin() == candidate.origin()
        && target.path() == candidate.path()
        && target
            .query_pairs()
            .filter(|(name, _)| !name.starts_with("__cf_chl_"))
            .all(|pair| candidate.query_pairs().any(|found| found == pair))
}

fn is_challenge_header(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("challenge")
}

fn is_html_type(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/html"))
}

fn is_attachment(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("attachment"))
}

#[cfg(test)]
#[path = "verification_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "verification_race_tests.rs"]
mod race_tests;
