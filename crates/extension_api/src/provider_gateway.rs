use std::path::Path;

use core_model::{HomePage, LoadDetails, MainPageEntry, PlayableLinks, ProviderInfo, SearchPage};

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

    /// Resolves a hoster URL through the extractor registry.
    fn extract(&self, url: &str, referer: Option<&str>) -> Result<PlayableLinks, GatewayError>;
}
