use std::path::Path;

use core_model::{HomePage, LoadDetails, MainPageEntry, PlayableLinks, ProviderInfo, SearchPage};
use extension_api::{GatewayError, ProviderGateway};

/// Stands in for the extension runtime when the sidecar is not set up, so the app still starts
/// and every call explains what is missing.
pub(crate) struct UnavailableGateway {
    reason: String,
}

impl UnavailableGateway {
    pub(crate) fn new(reason: String) -> Self {
        Self { reason }
    }

    fn error<T>(&self) -> Result<T, GatewayError> {
        Err(GatewayError::Unavailable(self.reason.clone()))
    }
}

impl ProviderGateway for UnavailableGateway {
    fn load_extension(
        &self,
        _path: &Path,
        _trusted: bool,
    ) -> Result<Vec<ProviderInfo>, GatewayError> {
        self.error()
    }

    fn search(
        &self,
        _provider: &str,
        _query: &str,
        _page: u32,
    ) -> Result<SearchPage, GatewayError> {
        self.error()
    }

    fn main_page(
        &self,
        _provider: &str,
        _page: u32,
        _entry: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        self.error()
    }

    fn load(&self, _provider: &str, _url: &str) -> Result<Option<LoadDetails>, GatewayError> {
        self.error()
    }

    fn load_links(&self, _provider: &str, _data: &str) -> Result<PlayableLinks, GatewayError> {
        self.error()
    }

    fn extract(&self, _url: &str, _referer: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        self.error()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_call_reports_the_reason() {
        let gateway = UnavailableGateway::new("sidecar missing".to_owned());
        let error = gateway.search("p", "q", 1).expect_err("must fail");
        assert_eq!(
            error,
            GatewayError::Unavailable("sidecar missing".to_owned())
        );
        assert!(gateway.load("p", "u").is_err());
    }
}
