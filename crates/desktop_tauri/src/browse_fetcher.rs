//! Fetching one TMDB Discover page for the Browse screen.

use std::io::Read;
use std::time::Duration;

use crate::browse_genres::{TmdbPageResponse, TMDB_API_KEY};

/// The whole request may take this long.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
/// Connecting to TMDB may take this long.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// A Discover page is a few KB; anything far bigger is not one.
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const USER_AGENT: &str = "CloudStream-rs";

/// Fetches a single TMDB Discover page.
pub trait BrowseFetcher: Send + Sync {
    /// Sends a GET request to `endpoint` with `params` and parses the JSON page response.
    fn fetch_page(
        &self,
        endpoint: &str,
        params: &[(String, String)],
    ) -> Result<TmdbPageResponse, String>;
}

/// Connection-pooled `ureq` implementation of [`BrowseFetcher`].
pub struct UreqBrowseFetcher {
    agent: ureq::Agent,
}

impl Default for UreqBrowseFetcher {
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

/// `message` with the TMDB key taken out, so errors can be shown and logged.
fn redact_key(message: &str) -> String {
    message.replace(TMDB_API_KEY, "[REDACTED]")
}

impl BrowseFetcher for UreqBrowseFetcher {
    fn fetch_page(
        &self,
        endpoint: &str,
        params: &[(String, String)],
    ) -> Result<TmdbPageResponse, String> {
        let mut request = self.agent.get(endpoint).header("User-Agent", USER_AGENT);
        for (name, value) in params {
            request = request.query(name, value);
        }
        let response = request
            .call()
            .map_err(|error| format!("HTTP request failed: {}", redact_key(&error.to_string())))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("TMDB returned HTTP {status}"));
        }
        let mut body = String::new();
        response
            .into_body()
            .into_reader()
            .take(MAX_RESPONSE_BYTES)
            .read_to_string(&mut body)
            .map_err(|error| format!("Reading response body failed: {error}"))?;
        serde_json::from_str(&body).map_err(|error| format!("JSON decode failed: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_key_removes_the_tmdb_key_from_an_error() {
        let redacted = redact_key(&format!("GET https://x/?api_key={TMDB_API_KEY} failed"));
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains(TMDB_API_KEY));
    }
}
