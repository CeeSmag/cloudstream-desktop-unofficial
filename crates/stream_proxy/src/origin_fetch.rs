//! Fetches an address of the stream the way the player would: following redirects, and trying
//! again where a host is known to need it.

use std::io::Read;
use std::time::Duration;

use crate::header_policy::HeaderPolicy;
use crate::resolve_reference::resolve_reference;
use crate::upstream::{Hop, HopResponse, Upstream, UpstreamError};
use crate::url_parts::is_http;

/// How many redirects are followed before giving up.
pub(crate) const MAX_REDIRECTS: usize = 5;

/// How long to wait before the one retry of a request that failed on the way.
const RETRY_DELAY: Duration = Duration::from_millis(250);

/// Statuses with which hosts refuse a `Range` request; the answer to the same request without it
/// is usually the whole file, which the player copes with.
const RANGE_REFUSED: &[u16] = &[400, 403, 405, 416];

/// The final answer: where it came from (relative addresses in it resolve against that), its
/// status, headers and body.
pub(crate) struct OriginResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) final_url: String,
    pub(crate) body: Box<dyn Read + Send>,
}

impl OriginResponse {
    /// The value of a response header, whatever its case.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(own, _)| own.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Asks `url`, then wherever it redirects to, with the headers the policy allows for each address.
fn follow(
    upstream: &dyn Upstream,
    policy: &HeaderPolicy,
    start: &str,
    range: Option<&str>,
) -> Result<OriginResponse, UpstreamError> {
    let mut url = start.to_owned();
    for _ in 0..=MAX_REDIRECTS {
        let hop = Hop {
            headers: policy.for_url(&url),
            range: range.map(str::to_owned),
            url: url.clone(),
        };
        let response = upstream.get(&hop)?;
        if !is_redirect(response.status) {
            return Ok(finished(response, url));
        }
        url = redirect_target(&response, &url)?;
    }
    Err(UpstreamError::TooManyRedirects(MAX_REDIRECTS))
}

fn finished(response: HopResponse, final_url: String) -> OriginResponse {
    OriginResponse {
        status: response.status,
        headers: response.headers,
        final_url,
        body: response.body,
    }
}

/// Where a redirect from `from` leads; only `http` and `https` addresses are followed.
fn redirect_target(response: &HopResponse, from: &str) -> Result<String, UpstreamError> {
    let location = response
        .header("location")
        .ok_or(UpstreamError::BadRedirect)?;
    resolve_reference(from, location)
        .filter(|target| is_http(target))
        .ok_or(UpstreamError::BadRedirect)
}

/// Fetches `url` (optionally only the part `range`), following redirects. A host that refuses the
/// range is asked again without it; a request that fails on the way is repeated once.
pub(crate) fn fetch(
    upstream: &dyn Upstream,
    policy: &HeaderPolicy,
    url: &str,
    range: Option<&str>,
) -> Result<OriginResponse, UpstreamError> {
    match follow(upstream, policy, url, range) {
        Ok(response) if range.is_some() && RANGE_REFUSED.contains(&response.status) => {
            tracing::debug!(
                status = response.status,
                "the host refused the range; asking for the whole file"
            );
            follow(upstream, policy, url, None)
        }
        Err(UpstreamError::Request { host, reason }) => {
            tracing::debug!(%host, %reason, "the request failed; trying once more");
            std::thread::sleep(RETRY_DELAY);
            follow(upstream, policy, url, range)
        }
        other => other,
    }
}

#[cfg(test)]
#[path = "origin_fetch_tests.rs"]
mod tests;
