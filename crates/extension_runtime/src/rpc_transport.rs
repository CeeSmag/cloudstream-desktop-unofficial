use std::time::Duration;

use extension_api::GatewayError;
use serde_json::Value;

/// A request/reply channel to the extension runtime.
pub trait RpcTransport: Send + Sync {
    /// Sends one request and waits at most `timeout` for its reply.
    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, GatewayError>;

    /// Lets calls in flight run until at least `extra` from now, past their own timeout. Used while
    /// the viewer works on something a call waits for. Transports without timeouts ignore it.
    fn extend_call_deadline(&self, extra: Duration) {
        let _ = extra;
    }

    /// Asks the transport to repeat this call whenever it has to start a fresh runtime (after a
    /// crash or timeout), so state built up by earlier calls, such as loaded extensions, is not
    /// lost. Transports that never restart ignore it.
    fn remember_for_restart(&self, method: &str, params: Value, timeout: Duration) {
        let _ = (method, params, timeout);
    }

    /// Like [`Self::remember_for_restart`] for a setting: only the latest value of `method` is
    /// repeated, so switching a setting on and off leaves it as the viewer last chose.
    fn remember_setting_for_restart(&self, method: &str, params: Value, timeout: Duration) {
        let _ = (method, params, timeout);
    }
}
