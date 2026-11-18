use std::path::Path;
use std::time::{Duration, Instant};

use core_model::{
    DiscoveredRepository, HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry,
    PlayableLinks, ProviderInfo, SearchPage,
};
use extension_api::{GatewayError, ProviderGateway};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::call_timeouts::{EXTENSION_LOAD_TIMEOUT, LINK_RESOLVE_TIMEOUT, PROVIDER_CALL_TIMEOUT};
use crate::RpcTransport;

/// `ProviderGateway` that speaks the sidecar JSON protocol over any `RpcTransport`.
pub struct RpcProviderGateway<T: RpcTransport> {
    transport: T,
}

impl<T: RpcTransport> RpcProviderGateway<T> {
    /// Wraps a transport.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    fn call_typed<R: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<R, GatewayError> {
        let target = call_target(&params);
        let started = Instant::now();
        let outcome = self.transport.call(method, params, timeout);
        log_call(method, &target, started.elapsed(), &outcome);
        serde_json::from_value(outcome?).map_err(|source| GatewayError::Decode {
            method: method.to_owned(),
            reason: source.to_string(),
        })
    }
}

/// A call that takes longer than this is reported as slow.
const SLOW_CALL: Duration = Duration::from_secs(10);

/// What a call is about, for the log: the provider, the extension file, or the address.
pub(crate) fn call_target(params: &Value) -> String {
    let field = |name: &str| params.get(name).and_then(Value::as_str);
    let target = field("provider")
        .or_else(|| field("path"))
        .or_else(|| field("url"))
        .unwrap_or_default();
    target
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(target)
        .to_owned()
}

/// Calls made in bulk or on a timer (home page rows, the poll for added repositories); they
/// are only logged at debug level unless they fail or are slow.
fn is_background_poll(method: &str) -> bool {
    matches!(
        method,
        "main_page" | "take_added_repositories" | "poll_link_search"
    )
}

/// Leaves a trace of every call: how long it took and whether it worked.
fn log_call(method: &str, target: &str, elapsed: Duration, outcome: &Result<Value, GatewayError>) {
    let elapsed_ms = elapsed.as_millis();
    match outcome {
        Err(error) => tracing::warn!(method, target, elapsed_ms, %error, "extension call failed"),
        Ok(_) if elapsed >= SLOW_CALL => {
            tracing::warn!(method, target, elapsed_ms, "extension call was slow");
        }
        Ok(_) if is_background_poll(method) => {
            tracing::debug!(method, target, elapsed_ms, "extension call finished");
        }
        Ok(_) => tracing::info!(method, target, elapsed_ms, "extension call finished"),
    }
}

impl<T: RpcTransport> ProviderGateway for RpcProviderGateway<T> {
    fn load_extension(
        &self,
        path: &Path,
        trusted: bool,
    ) -> Result<Vec<ProviderInfo>, GatewayError> {
        let params = json!({ "path": path.to_string_lossy(), "trusted": trusted });
        let providers =
            self.call_typed("load_extension", params.clone(), EXTENSION_LOAD_TIMEOUT)?;
        // A restarted runtime starts empty; have the transport load this extension again.
        self.transport
            .remember_for_restart("load_extension", params, EXTENSION_LOAD_TIMEOUT);
        Ok(providers)
    }

    fn unload_extension(&self, path: &Path) -> Result<(), GatewayError> {
        let params = json!({ "path": path.to_string_lossy() });
        self.call_typed::<Value>("unload_extension", params.clone(), PROVIDER_CALL_TIMEOUT)
            .map(drop)?;
        // An uninstalled plugin must not come back if the sidecar restarts.
        self.transport.forget_for_restart("load_extension", &params);
        Ok(())
    }

    fn search(&self, provider: &str, query: &str, page: u32) -> Result<SearchPage, GatewayError> {
        let params = json!({ "provider": provider, "query": query, "page": page });
        self.call_typed("search", params, PROVIDER_CALL_TIMEOUT)
    }

    fn main_page(
        &self,
        provider: &str,
        page: u32,
        entry: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        let params = json!({
            "provider": provider, "page": page, "name": entry.name,
            "data": entry.data, "horizontal": entry.horizontal_images,
        });
        self.call_typed("main_page", params, PROVIDER_CALL_TIMEOUT)
    }

    fn load(&self, provider: &str, url: &str) -> Result<Option<LoadDetails>, GatewayError> {
        let params = json!({ "provider": provider, "url": url });
        self.call_typed("load", params, PROVIDER_CALL_TIMEOUT)
    }

    fn load_links(&self, provider: &str, data: &str) -> Result<PlayableLinks, GatewayError> {
        let params = json!({ "provider": provider, "data": data });
        self.call_typed("load_links", params, LINK_RESOLVE_TIMEOUT)
    }

    fn start_link_search(&self, provider: &str, data: &str) -> Result<String, GatewayError> {
        let params = json!({ "provider": provider, "data": data });
        self.call_typed("start_link_search", params, PROVIDER_CALL_TIMEOUT)
    }

    fn poll_link_search(
        &self,
        id: &str,
        cursor: LinkSearchCursor,
        wait: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        let params = json!({
            "id": id,
            "fromLink": cursor.links,
            "fromSubtitle": cursor.subtitles,
            "waitMs": wait.as_millis(),
        });
        self.call_typed("poll_link_search", params, wait + PROVIDER_CALL_TIMEOUT)
    }

    fn cancel_link_search(&self, id: &str) -> Result<(), GatewayError> {
        let params = json!({ "id": id });
        self.call_typed::<Value>("cancel_link_search", params, PROVIDER_CALL_TIMEOUT)
            .map(drop)
    }

    fn extract(&self, url: &str, referer: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        let params = json!({ "url": url, "referer": referer });
        self.call_typed("extract", params, PROVIDER_CALL_TIMEOUT)
    }

    fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, GatewayError> {
        self.call_typed("take_added_repositories", json!({}), PROVIDER_CALL_TIMEOUT)
    }

    fn extend_call_deadline(&self, extra: Duration) {
        self.transport.extend_call_deadline(extra);
    }

    fn shutdown(&self) {
        self.transport.shutdown();
    }

    fn set_debug_logging(&self, enabled: bool) -> Result<(), GatewayError> {
        let params = json!({ "enabled": enabled });
        // A restarted runtime starts with debug logging off; the choice is sent again.
        self.transport.remember_setting_for_restart(
            "set_debug_logging",
            params.clone(),
            PROVIDER_CALL_TIMEOUT,
        );
        self.call_typed::<Value>("set_debug_logging", params, PROVIDER_CALL_TIMEOUT)
            .map(drop)
    }
}

#[cfg(test)]
#[path = "rpc_provider_gateway_tests.rs"]
mod tests;
