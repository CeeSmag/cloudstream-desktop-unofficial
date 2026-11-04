//! Asking a database over HTTP.

use std::io::Read;
use std::time::Duration;

/// How long one request may take in all; the lookup must never hold the player up.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// The largest answer accepted; a real one is a few hundred bytes.
const MAX_BYTES: u64 = 256 * 1024;
const USER_AGENT: &str = "CloudStream-rs";

/// Why a lookup failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkipError {
    /// The database answered with an error status.
    #[error("HTTP {status}")]
    Status {
        /// The status code.
        status: u16,
    },
    /// The database could not be reached or stopped answering.
    #[error("{reason}")]
    Network {
        /// What the network layer said.
        reason: String,
    },
    /// The answer was not what the database documents.
    #[error("unreadable answer: {reason}")]
    Invalid {
        /// What was wrong with it.
        reason: String,
    },
}

/// Gets the text a URL answers with.
pub trait SkipFetcher: Send + Sync {
    /// Fetches `url` and returns the body.
    fn get_text(&self, url: &str) -> Result<String, SkipError>;
}

/// [`SkipFetcher`] backed by `ureq`.
pub struct UreqSkipFetcher {
    agent: ureq::Agent,
}

impl Default for UreqSkipFetcher {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl SkipFetcher for UreqSkipFetcher {
    #[tracing::instrument(level = "debug", skip_all, err)]
    fn get_text(&self, url: &str) -> Result<String, SkipError> {
        let network = |error: String| SkipError::Network { reason: error };
        let response = self
            .agent
            .get(url)
            .header("User-Agent", USER_AGENT)
            .call()
            .map_err(|error| network(error.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(SkipError::Status { status });
        }
        let mut body = String::new();
        response
            .into_body()
            .into_reader()
            .take(MAX_BYTES)
            .read_to_string(&mut body)
            .map_err(|error| network(error.to_string()))?;
        Ok(body)
    }
}
