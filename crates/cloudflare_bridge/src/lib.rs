//! Cloudflare challenge file exchange for the egui product (no main WebView chrome).
//!
//! The sidecar writes `request-*.txt` under `<data>/sidecar/cloudflare`. This crate parses those
//! files, can open the challenge URL in the system browser, and writes `result-*.txt` answers.
#![forbid(unsafe_code)]

mod exchange;
mod watch;

pub use exchange::{
    clear_folder, format_result, parse_request, pending_requests, write_result, Outcome,
    VerifyRequest,
};
pub use watch::{spawn_watcher, CloudflareEvent, WatchHandle};
