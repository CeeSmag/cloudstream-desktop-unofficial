//! One request to one address of the stream's hosts. Redirects and retries are the caller's job
//! (`origin_fetch`), so that they can be tested without a network.

use std::io::Read;

/// One request: the headers to send and, when the player asked for a part, its `Range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hop {
    pub(crate) url: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) range: Option<String>,
}

/// What a host answered; the body is read by the caller, as far as it wants.
pub(crate) struct HopResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Box<dyn Read + Send>,
}

impl HopResponse {
    /// The value of a response header, whatever its case.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(own, _)| own.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Anything that can make one request. Error statuses are answers, not errors.
pub(crate) trait Upstream: Send + Sync {
    /// Sends `hop` and returns the answer's status, headers and body.
    fn get(&self, hop: &Hop) -> Result<HopResponse, UpstreamError>;
}

/// Why a stream's host could not be asked.
#[derive(Debug, thiserror::Error)]
pub(crate) enum UpstreamError {
    /// The connection, the sending or the waiting failed (the host is named, never the whole address).
    #[error("the request to {host} failed: {reason}")]
    Request {
        /// Host that was asked.
        host: String,
        /// What went wrong, as the HTTP client says.
        reason: String,
    },
    /// The host kept redirecting.
    #[error("more than {0} redirects")]
    TooManyRedirects(usize),
    /// A redirect without a usable `Location`, or to an address that is not `http` or `https`.
    #[error("a redirect that cannot be followed")]
    BadRedirect,
}
