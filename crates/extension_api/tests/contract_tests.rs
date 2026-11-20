//! Contract tests covering default `ProviderGateway` trait methods and `GatewayError` formatting.

use std::path::Path;
use std::time::Duration;

use core_model::{
    HomePage, LinkSearchCursor, LoadDetails, MainPageEntry, PlayableLinks, ProviderInfo, SearchPage,
};
use extension_api::{GatewayError, ProviderGateway};

struct MinimalGateway;

impl ProviderGateway for MinimalGateway {
    fn load_extension(
        &self,
        _path: &Path,
        _trusted: bool,
    ) -> Result<Vec<ProviderInfo>, GatewayError> {
        Ok(Vec::new())
    }

    fn search(
        &self,
        _provider: &str,
        _query: &str,
        _page: u32,
    ) -> Result<SearchPage, GatewayError> {
        Ok(SearchPage::default())
    }

    fn main_page(
        &self,
        _provider: &str,
        _page: u32,
        _entry: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        Ok(None)
    }

    fn load(&self, _provider: &str, _url: &str) -> Result<Option<LoadDetails>, GatewayError> {
        Ok(None)
    }

    fn load_links(&self, _provider: &str, _data: &str) -> Result<PlayableLinks, GatewayError> {
        Ok(PlayableLinks::default())
    }

    fn extract(&self, _url: &str, _referer: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        Ok(PlayableLinks::default())
    }
}

#[test]
fn default_provider_gateway_methods_behave_safely() {
    let gw = MinimalGateway;

    assert!(gw.unload_extension(Path::new("demo.cs3")).is_ok());

    let start_err = gw
        .start_link_search("Demo", "ep1")
        .expect_err("default runtime does not support streaming search");
    assert!(matches!(start_err, GatewayError::Unavailable(_)));

    let poll_err = gw
        .poll_link_search(
            "id-1",
            LinkSearchCursor::default(),
            Duration::from_millis(5),
        )
        .expect_err("default runtime has no running link searches");
    assert!(matches!(poll_err, GatewayError::Unavailable(_)));

    assert!(gw.cancel_link_search("id-1").is_ok());
    assert!(gw
        .take_added_repositories()
        .expect("default returns empty")
        .is_empty());
    gw.extend_call_deadline(Duration::from_secs(10));
    gw.shutdown();
    assert!(gw.set_debug_logging(true).is_ok());
    assert!(gw.set_debug_logging(false).is_ok());
    gw.set_resident_worker_limit(Some(1));
    gw.set_resident_worker_limit(None);
}

#[test]
fn gateway_error_variants_format_with_context() {
    assert_eq!(
        GatewayError::Unavailable("missing java".into()).to_string(),
        "extension runtime is not available: missing java"
    );
    assert_eq!(
        GatewayError::Transport("broken pipe".into()).to_string(),
        "extension runtime transport failed: broken pipe"
    );
    assert_eq!(
        GatewayError::Decode {
            method: "load".into(),
            reason: "bad json".into(),
        }
        .to_string(),
        "cannot decode reply of 'load': bad json"
    );
    assert_eq!(
        GatewayError::Timeout {
            method: "load_links".into(),
            seconds: 150,
        }
        .to_string(),
        "extension call 'load_links' timed out after 150s"
    );
    assert_eq!(
        GatewayError::Remote {
            kind: "IOException".into(),
            message: "refused".into(),
        }
        .to_string(),
        "extension reported IOException: refused"
    );
}
