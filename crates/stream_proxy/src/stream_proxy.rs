use std::sync::Arc;

use crate::entry_kind::EntryKind;
use crate::proxy_config::ProxyConfig;
use crate::proxy_error::ProxyError;
use crate::proxy_server::ProxyServer;
use crate::proxy_url::proxied_url;
use crate::request_handler::Handler;
use crate::session_registry::SessionRegistry;
use crate::stream_session::FIRST_PLAYLIST_ID;
use crate::stream_spec::StreamSpec;
use crate::upstream::Upstream;
use crate::ureq_upstream::UreqUpstream;
use crate::url_parts::host_of;

/// The local proxy: opens streams and serves them to the player on this PC.
///
/// At most two streams are served at a time, the newest and the one it replaced (see
/// `session_registry`). Dropping the proxy shuts it down.
pub struct StreamProxy {
    registry: Arc<SessionRegistry>,
    server: ProxyServer,
}

impl StreamProxy {
    /// Starts the proxy on a free port of this PC's loopback address.
    pub fn start(config: ProxyConfig) -> Result<Self, ProxyError> {
        Self::start_with(config, Arc::new(UreqUpstream::new()))
    }

    /// Starts the proxy with another way of asking the stream's hosts (tests).
    pub(crate) fn start_with(
        config: ProxyConfig,
        upstream: Arc<dyn Upstream>,
    ) -> Result<Self, ProxyError> {
        let registry = Arc::new(SessionRegistry::default());
        let for_handler = Arc::clone(&registry);
        let server = ProxyServer::start(
            move |port| Handler::new(for_handler, upstream, port),
            config,
        )?;
        Ok(Self { registry, server })
    }

    /// Opens a stream and returns the address the player should open instead of `spec.url`.
    /// Only `http` and `https` streams are opened.
    pub fn open(&self, spec: &StreamSpec) -> Result<String, ProxyError> {
        if self.server.is_stopped() {
            return Err(ProxyError::ShutDown);
        }
        let session = self.registry.open(spec)?;
        tracing::info!(
            host = %host_of(&spec.url).unwrap_or_default(),
            "a stream is served through the proxy"
        );
        Ok(proxied_url(
            self.server.port(),
            session.token(),
            FIRST_PLAYLIST_ID,
            EntryKind::Playlist.extension(&spec.url),
        ))
    }

    /// Closes every stream: what is being sent stops and later requests are refused.
    pub fn close(&self) {
        self.registry.close();
    }

    /// The port the proxy listens on.
    pub fn port(&self) -> u16 {
        self.server.port()
    }

    /// Closes every stream, stops listening and waits (for a limited time) for the connections
    /// being served. Safe to call more than once.
    pub fn shutdown(&self) {
        self.registry.close();
        self.server.shutdown();
    }
}

#[cfg(test)]
impl StreamProxy {
    /// How many connections are being served.
    pub(crate) fn open_connections(&self) -> usize {
        self.server.open_connections()
    }
}

impl Drop for StreamProxy {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
#[path = "stream_proxy_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stream_proxy_refusal_tests.rs"]
mod refusal_tests;

#[cfg(test)]
#[path = "stream_proxy_race_tests.rs"]
mod race_tests;
