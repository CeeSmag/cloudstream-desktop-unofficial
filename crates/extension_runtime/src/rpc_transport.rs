use std::time::Duration;

use extension_api::GatewayError;
use serde_json::Value;

/// A request/reply channel to the extension runtime.
pub trait RpcTransport: Send + Sync {
    /// Sends one request and waits at most `timeout` for its reply.
    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, GatewayError>;
}
