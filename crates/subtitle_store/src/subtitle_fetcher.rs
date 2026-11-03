//! Downloading a subtitle file from its host.

use std::io::Read;
use std::time::Duration;

/// How long one download may take in all; a host slower than this is given up on.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The largest subtitle accepted; real ones are a few hundred kilobytes at most.
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) CloudStream";

/// Why a download failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    /// The host answered with an error status.
    #[error("HTTP {status}")]
    Status {
        /// The status code.
        status: u16,
    },
    /// The host could not be reached or stopped answering.
    #[error("{reason}")]
    Network {
        /// What the network layer said.
        reason: String,
    },
    /// The answer was bigger than any subtitle should be.
    #[error("the file is larger than {MAX_BYTES} bytes")]
    TooLarge,
}

impl FetchError {
    /// Whether trying again may help: a network failure, a busy or failing host. A missing or
    /// forbidden file stays missing or forbidden.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Network { .. } => true,
            Self::Status { status } => *status == 408 || *status == 429 || *status >= 500,
            Self::TooLarge => false,
        }
    }
}

/// Downloads the body of a URL.
pub trait SubtitleFetcher: Send + Sync {
    /// Fetches `url`, sending `headers`, and returns the body.
    fn fetch(&self, url: &str, headers: &[(String, String)]) -> Result<Vec<u8>, FetchError>;
}

/// [`SubtitleFetcher`] backed by `ureq`.
pub struct UreqSubtitleFetcher {
    agent: ureq::Agent,
}

impl Default for UreqSubtitleFetcher {
    fn default() -> Self {
        Self::with_timeout(REQUEST_TIMEOUT)
    }
}

impl UreqSubtitleFetcher {
    /// A fetcher that gives up on a download after `timeout` in all.
    pub fn with_timeout(timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .timeout_connect(Some(CONNECT_TIMEOUT.min(timeout)))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl SubtitleFetcher for UreqSubtitleFetcher {
    #[tracing::instrument(level = "debug", skip_all, fields(url = ?url), err)]
    fn fetch(&self, url: &str, headers: &[(String, String)]) -> Result<Vec<u8>, FetchError> {
        let network = |error: String| FetchError::Network { reason: error };
        let mut request = self.agent.get(url).header("User-Agent", USER_AGENT);
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.call().map_err(|error| network(error.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(FetchError::Status { status });
        }
        let mut body = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| network(error.to_string()))?;
        if u64::try_from(body.len()).unwrap_or(u64::MAX) > MAX_BYTES {
            return Err(FetchError::TooLarge);
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_failures_that_may_pass_are_worth_retrying() {
        let status = |status| FetchError::Status { status };
        assert!(FetchError::Network {
            reason: "timed out".into()
        }
        .is_transient());
        assert!(
            status(429).is_transient() && status(503).is_transient() && status(408).is_transient()
        );
        assert!(!status(404).is_transient() && !status(403).is_transient());
        assert!(!FetchError::TooLarge.is_transient());
    }
}
