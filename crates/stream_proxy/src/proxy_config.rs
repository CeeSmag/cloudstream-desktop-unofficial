use std::time::Duration;

/// The limits and timeouts of the proxy; the defaults suit a player on the same PC.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// How many connections are served at once; more are refused with 503 and the player asks
    /// again. A player opens a few at a time.
    pub max_connections: usize,
    /// How long a player may take to send its request.
    pub request_timeout: Duration,
    /// How long the player may take to accept what is sent to it.
    pub write_timeout: Duration,
    /// How long `shutdown` waits for connections that are still being served.
    pub shutdown_wait: Duration,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            max_connections: 64,
            request_timeout: Duration::from_secs(10),
            write_timeout: Duration::from_secs(30),
            shutdown_wait: Duration::from_secs(3),
        }
    }
}
