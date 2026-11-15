//! A small local HTTP proxy for HLS streams.
//!
//! The player is given `http://127.0.0.1:<port>/...` instead of the stream's own address. The
//! proxy fetches every playlist, segment and key with the provider's headers and referer, resolves
//! relative addresses the way the player would, and hands the player rewritten playlists that point
//! back at itself. Nothing here knows about the player, the UI or the extensions.

#![forbid(unsafe_code)]
// The pieces are built and tested one at a time; the server that uses them all comes last.
#![allow(dead_code)]

mod entry_kind;
mod header_policy;
mod hls_rewrite;
mod proxy_error;
mod proxy_url;
mod resolve_reference;
mod session_registry;
mod session_token;
mod stream_session;
mod stream_spec;
mod url_parts;
mod url_table;

pub use stream_spec::StreamSpec;
