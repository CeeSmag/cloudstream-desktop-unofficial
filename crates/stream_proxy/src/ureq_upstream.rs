//! `Upstream` over `ureq`, with timeouts on every step and no redirects of its own.

use std::time::Duration;

use crate::upstream::{Hop, HopResponse, Upstream, UpstreamError};
use crate::url_parts::host_of;

/// How long a connection may take to open.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a name lookup may take.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the host may stay silent while answering or while sending the body. Pieces of a
/// stream are small and a host that stops for longer than this has failed.
const SILENCE_TIMEOUT: Duration = Duration::from_secs(20);

/// Makes the requests with `ureq`.
pub(crate) struct UreqUpstream {
    agent: ureq::Agent,
}

impl UreqUpstream {
    /// An agent that honours the system's proxy settings, like the player does.
    pub(crate) fn new() -> Self {
        Self::from_builder(ureq::Agent::config_builder())
    }

    /// An agent that never uses a proxy (tests talk to a server on this PC).
    #[cfg(test)]
    pub(crate) fn without_proxy() -> Self {
        Self::from_builder(ureq::Agent::config_builder().proxy(None))
    }

    fn from_builder(builder: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>) -> Self {
        let config = builder
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_resolve(Some(RESOLVE_TIMEOUT))
            .timeout_per_read(Some(SILENCE_TIMEOUT))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Upstream for UreqUpstream {
    fn get(&self, hop: &Hop) -> Result<HopResponse, UpstreamError> {
        let mut request = self.agent.get(hop.url.as_str());
        for (name, value) in &hop.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        if let Some(range) = &hop.range {
            request = request.header("Range", range.as_str());
        }
        let response = request.call().map_err(|error| UpstreamError::Request {
            host: host_of(&hop.url).unwrap_or_default(),
            reason: error.to_string(),
        })?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
            })
            .collect();
        Ok(HopResponse {
            status,
            headers,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}
