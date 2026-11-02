use std::path::Path;
use std::time::Duration;

use core_model::{HomePage, LoadDetails, MainPageEntry, PlayableLinks, ProviderInfo, SearchPage};
use extension_api::{GatewayError, ProviderGateway};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::call_timeouts::{EXTENSION_LOAD_TIMEOUT, PROVIDER_CALL_TIMEOUT};
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
        tracing::debug!(method, "calling extension runtime");
        let value = self.transport.call(method, params, timeout)?;
        serde_json::from_value(value).map_err(|source| GatewayError::Decode {
            method: method.to_owned(),
            reason: source.to_string(),
        })
    }
}

impl<T: RpcTransport> ProviderGateway for RpcProviderGateway<T> {
    fn load_extension(
        &self,
        path: &Path,
        trusted: bool,
    ) -> Result<Vec<ProviderInfo>, GatewayError> {
        let params = json!({ "path": path.to_string_lossy(), "trusted": trusted });
        self.call_typed("load_extension", params, EXTENSION_LOAD_TIMEOUT)
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
        self.call_typed("load_links", params, PROVIDER_CALL_TIMEOUT)
    }

    fn extract(&self, url: &str, referer: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        let params = json!({ "url": url, "referer": referer });
        self.call_typed("extract", params, PROVIDER_CALL_TIMEOUT)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Returns canned values and records every call.
    struct ScriptedTransport {
        reply: Result<Value, GatewayError>,
        calls: Mutex<Vec<(String, Value)>>,
    }

    impl ScriptedTransport {
        fn replying(reply: Result<Value, GatewayError>) -> Self {
            Self {
                reply,
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl RpcTransport for ScriptedTransport {
        fn call(
            &self,
            method: &str,
            params: Value,
            _timeout: Duration,
        ) -> Result<Value, GatewayError> {
            self.calls
                .lock()
                .expect("lock")
                .push((method.to_owned(), params));
            self.reply.clone()
        }
    }

    #[test]
    fn search_sends_provider_query_and_page_and_decodes_items() {
        let reply = json!({"items":[{"name":"Apollo","url":"u"}],"hasNext":true});
        let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Ok(reply)));

        let page = gateway
            .search("Archive", "moon", 2)
            .expect("search succeeds");

        assert_eq!(page.items[0].name, "Apollo");
        assert!(page.has_next);
        let calls = gateway.transport.calls.lock().expect("lock");
        assert_eq!(calls[0].0, "search");
        assert_eq!(
            calls[0].1,
            json!({"provider":"Archive","query":"moon","page":2})
        );
    }

    #[test]
    fn load_returns_none_when_runtime_replies_null() {
        let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Ok(Value::Null)));
        assert_eq!(gateway.load("Archive", "u").expect("load succeeds"), None);
    }

    #[test]
    fn remote_failure_is_propagated_unchanged() {
        let failure = GatewayError::Remote {
            kind: "NotImplementedError".into(),
            message: String::new(),
        };
        let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Err(failure.clone())));
        assert_eq!(gateway.search("p", "q", 1), Err(failure));
    }

    #[test]
    fn malformed_reply_becomes_decode_error_naming_the_method() {
        let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Ok(json!("not a page"))));
        match gateway.search("p", "q", 1) {
            Err(GatewayError::Decode { method, .. }) => assert_eq!(method, "search"),
            other => panic!("expected decode error, got {other:?}"),
        }
    }

    #[test]
    fn extract_sends_null_referer_when_absent() {
        let gateway =
            RpcProviderGateway::new(ScriptedTransport::replying(Ok(json!({"success":false}))));
        gateway
            .extract("https://h.test/v", None)
            .expect("extract succeeds");
        let calls = gateway.transport.calls.lock().expect("lock");
        assert_eq!(calls[0].1, json!({"url":"https://h.test/v","referer":null}));
    }
}
