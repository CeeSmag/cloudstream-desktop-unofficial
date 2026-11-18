//! Downloading one poster image.

use std::io::Read;
use std::time::Duration;

use crate::ArtworkError;

/// How long one download may take in all.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The largest image accepted; a poster at full TMDB size is a few megabytes.
pub const MAX_IMAGE_BYTES: u64 = 12 * 1024 * 1024;
/// Some image hosts refuse clients that do not look like a browser.
const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
/// Only formats this build decodes (no AVIF), so a host that negotiates sends a readable one.
const ACCEPT: &str = "image/webp,image/png,image/jpeg,image/gif;q=0.9,*/*;q=0.5";

/// Downloads the bytes of an image.
pub trait ArtworkFetcher: Send + Sync {
    /// Fetches `url`, sending `referer` when given (hosts that refuse hot-linking want it).
    fn fetch(&self, url: &str, referer: Option<&str>) -> Result<Vec<u8>, ArtworkError>;
}

/// [`ArtworkFetcher`] backed by `ureq`.
pub struct UreqArtworkFetcher {
    agent: ureq::Agent,
}

impl Default for UreqArtworkFetcher {
    fn default() -> Self {
        Self::with_timeout(REQUEST_TIMEOUT)
    }
}

impl UreqArtworkFetcher {
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

impl ArtworkFetcher for UreqArtworkFetcher {
    fn fetch(&self, url: &str, referer: Option<&str>) -> Result<Vec<u8>, ArtworkError> {
        let mut request = self
            .agent
            .get(url)
            .header("User-Agent", USER_AGENT)
            .header("Accept", ACCEPT);
        if let Some(referer) = referer.filter(|value| !value.is_empty()) {
            request = request.header("Referer", referer);
        }
        let response = request
            .call()
            .map_err(|error| ArtworkError::Network(error.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(ArtworkError::Status(status));
        }
        let mut bytes = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(MAX_IMAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| ArtworkError::Network(error.to_string()))?;
        if bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err(ArtworkError::TooLarge(MAX_IMAGE_BYTES));
        }
        Ok(bytes)
    }
}
