use std::path::Path;
use std::time::Duration;

use core_model::{
    DiscoveredRepository, HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry,
    PlayableLinks, ProviderInfo, SearchPage,
};

use crate::GatewayError;

/// The app's only view of extensions: load them, then query their providers.
///
/// Implementations decide how extensions actually run (JVM sidecar, Wasm, ...).
pub trait ProviderGateway: Send + Sync {
    /// Loads an extension file and returns the providers it registered.
    fn load_extension(&self, path: &Path, trusted: bool)
        -> Result<Vec<ProviderInfo>, GatewayError>;

    /// Searches one provider; pages start at 1.
    fn search(&self, provider: &str, query: &str, page: u32) -> Result<SearchPage, GatewayError>;

    /// Loads one home page category of a provider.
    fn main_page(
        &self,
        provider: &str,
        page: u32,
        entry: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError>;

    /// Loads the details of a title; `None` when the provider returns nothing.
    fn load(&self, provider: &str, url: &str) -> Result<Option<LoadDetails>, GatewayError>;

    /// Resolves the playable streams for a movie/episode payload.
    fn load_links(&self, provider: &str, data: &str) -> Result<PlayableLinks, GatewayError>;

    /// Starts looking for the streams of a movie or episode and returns the id of the search at
    /// once; [`ProviderGateway::poll_link_search`] then reports what it finds while it runs.
    /// Runtimes that can only answer when the whole search is over return an error here, and the
    /// caller falls back to [`ProviderGateway::load_links`].
    fn start_link_search(&self, provider: &str, data: &str) -> Result<String, GatewayError> {
        let _ = (provider, data);
        Err(GatewayError::Unavailable(
            "this runtime cannot report streams while it is still searching".to_owned(),
        ))
    }

    /// What search `id` found after `cursor`, waiting up to `wait` for news when there is none yet.
    fn poll_link_search(
        &self,
        id: &str,
        cursor: LinkSearchCursor,
        wait: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        let _ = (id, cursor, wait);
        Err(GatewayError::Unavailable(
            "this runtime has no running link searches".to_owned(),
        ))
    }

    /// Stops search `id`, which is no longer wanted.
    fn cancel_link_search(&self, id: &str) -> Result<(), GatewayError> {
        let _ = id;
        Ok(())
    }

    /// Resolves a hoster URL through the extractor registry.
    fn extract(&self, url: &str, referer: Option<&str>) -> Result<PlayableLinks, GatewayError>;

    /// Hands over, and forgets, the repositories extensions asked the app to add since the last
    /// call. Runtimes that cannot report any return an empty list.
    fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, GatewayError> {
        Ok(Vec::new())
    }

    /// Lets calls in flight run until at least `extra` from now, past their own timeout: the viewer
    /// is busy with something a call waits for (passing a Cloudflare check in a browser window).
    /// Runtimes whose calls do not time out ignore it.
    fn extend_call_deadline(&self, extra: Duration) {
        let _ = extra;
    }

    /// Tells the runtime whether debug logging is on; while it is off the runtime and its
    /// plugins keep their detailed output to themselves. Runtimes without such a switch ignore it.
    fn set_debug_logging(&self, enabled: bool) -> Result<(), GatewayError> {
        let _ = enabled;
        Ok(())
    }
}
