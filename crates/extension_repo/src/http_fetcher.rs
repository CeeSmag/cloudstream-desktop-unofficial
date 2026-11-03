use std::io::Read;
use std::time::Duration;

use crate::RepoError;

/// Largest download accepted: repository files are tiny, plugins are small too.
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
/// Network timeout for repository traffic.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Downloads a URL into memory.
pub trait HttpFetcher: Send + Sync {
    /// Fetches `url` and returns the response body.
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RepoError>;
}

impl<T: HttpFetcher + ?Sized> HttpFetcher for std::sync::Arc<T> {
    #[tracing::instrument(level = "debug", skip_all, fields(url = ?url), err)]
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RepoError> {
        (**self).get_bytes(url)
    }
}

/// `HttpFetcher` backed by `ureq`.
pub struct UreqFetcher {
    agent: ureq::Agent,
}

impl Default for UreqFetcher {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl HttpFetcher for UreqFetcher {
    #[tracing::instrument(level = "debug", skip_all, fields(url = ?url), err)]
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RepoError> {
        let fail = |reason: String| RepoError::Download {
            url: url.to_owned(),
            reason,
        };
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| fail(error.to_string()))?;
        let mut bytes = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(MAX_DOWNLOAD_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|error| fail(error.to_string()))?;
        Ok(bytes)
    }
}
