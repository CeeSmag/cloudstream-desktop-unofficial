use std::time::Duration;

use extension_api::GatewayError;
use serde_json::Value;

/// A request/reply channel to the extension runtime.
pub trait RpcTransport: Send + Sync {
    /// Sends one request and waits at most `timeout` for its reply.
    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, GatewayError>;

    /// Asks the transport to repeat this call whenever it has to start a fresh runtime (after a
    /// crash or timeout), so state built up by earlier calls, such as loaded extensions, is not
    /// lost. Transports that never restart ignore it.
    fn remember_for_restart(&self, method: &str, params: Value, timeout: Duration) {
        let _ = (method, params, timeout);
    }
}
