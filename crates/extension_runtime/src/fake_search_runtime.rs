//! An in-process stand-in for the sidecar that only knows the streaming link search, for tests that
//! drive the app's whole opening flow without starting a process.

use std::sync::Arc;
use std::time::Duration;

use core_model::LinkSearchCursor;
use extension_api::GatewayError;
use serde_json::Value;

use crate::fake_link_search::FakeLinkSearches;
use crate::RpcTransport;

/// Answers `start_link_search`, `poll_link_search` and `cancel_link_search` from a
/// [`FakeLinkSearches`], and refuses everything else.
pub struct FakeSearchRuntime {
    searches: Arc<FakeLinkSearches>,
}

impl FakeSearchRuntime {
    /// A runtime over `searches`, which the test keeps to read what was started and cancelled.
    pub fn new(searches: Arc<FakeLinkSearches>) -> Self {
        Self { searches }
    }
}

fn text(params: &Value, name: &str) -> String {
    params
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn number(params: &Value, name: &str) -> u64 {
    params.get(name).and_then(Value::as_u64).unwrap_or_default()
}

impl RpcTransport for FakeSearchRuntime {
    fn call(&self, method: &str, params: Value, _timeout: Duration) -> Result<Value, GatewayError> {
        match method {
            "start_link_search" => Ok(Value::String(self.searches.start(&text(&params, "data")))),
            "poll_link_search" => self
                .searches
                .poll(
                    &text(&params, "id"),
                    LinkSearchCursor {
                        links: usize::try_from(number(&params, "fromLink")).unwrap_or(0),
                        subtitles: usize::try_from(number(&params, "fromSubtitle")).unwrap_or(0),
                    },
                    Duration::from_millis(number(&params, "waitMs")),
                )
                .map_err(|message| GatewayError::Remote {
                    kind: "IllegalStateException".to_owned(),
                    message,
                }),
            "cancel_link_search" => {
                self.searches.cancel(&text(&params, "id"));
                Ok(Value::Null)
            }
            other => Err(GatewayError::Unavailable(format!(
                "the fake runtime does not know {other}"
            ))),
        }
    }
}
