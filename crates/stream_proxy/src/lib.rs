//! A small local HTTP proxy for HLS streams.
//!
//! The player is given `http://127.0.0.1:<port>/...` instead of the stream's own address. The
//! proxy fetches every playlist, segment and key with the provider's headers and referer, resolves
//! relative addresses the way the player would, and hands the player rewritten playlists that point
//! back at itself. Nothing here knows about the player, the UI or the extensions.

#![forbid(unsafe_code)]

mod connection_gate;
mod entry_kind;
#[cfg(test)]
mod fake_origin;
#[cfg(test)]
mod fake_upstream;
mod header_policy;
mod hls_rewrite;
mod http_request;
mod http_response;
mod origin_fetch;
mod playlist_response;
mod proxy_config;
mod proxy_error;
mod proxy_server;
#[cfg(test)]
mod proxy_test_support;
mod proxy_url;
mod relay_body;
mod request_handler;
mod resolve_reference;
mod session_registry;
mod session_token;
mod stream_proxy;
mod stream_session;
mod stream_spec;
mod upstream;
mod ureq_upstream;
mod url_parts;
mod url_table;

pub use proxy_config::ProxyConfig;
pub use proxy_error::ProxyError;
pub use stream_proxy::StreamProxy;
pub use stream_spec::StreamSpec;
