//! Evidence from the browser's main navigation and the verification lifecycle.

use url::Url;

pub(crate) type Cookies = Vec<(String, String)>;

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

    fn advances(&self) -> bool {
        !self.challenge && (200..400).contains(&self.status)
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
    Passed(Cookies),
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
        if response.leaves_origin(&self.target)
            || (!response.challenge && !(300..400).contains(&response.status))
        {
            return Decision::Unconfirmed;
        }
        Decision::Wait
    }

    fn is_passage(&self, response: &MainResponse, cookies: &Cookies) -> bool {
        response.advances()
            && matches_target(&self.target, &response.url)
            && cookies.iter().any(|cookie| {
                cookie.0 == "cf_clearance"
                    && !cookie.1.is_empty()
                    && (!self.encountered_challenge || !self.baseline.contains(cookie))
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

pub(crate) fn cookie_matches(target: &Url, domain: &str, path: &str, secure: bool) -> bool {
    let Some(host) = target.host_str() else {
        return false;
    };
    let domain_matches = if let Some(suffix) = domain.strip_prefix('.') {
        host.eq_ignore_ascii_case(suffix)
            || host
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'))
    } else {
        host.eq_ignore_ascii_case(domain)
    };
    let request_path = target.path();
    let path_matches = request_path == path
        || request_path
            .strip_prefix(path)
            .is_some_and(|tail| path.ends_with('/') || tail.starts_with('/'));
    domain_matches && path_matches && (!secure || target.scheme() == "https")
}

#[cfg(test)]
#[path = "verification_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "verification_race_tests.rs"]
mod race_tests;
