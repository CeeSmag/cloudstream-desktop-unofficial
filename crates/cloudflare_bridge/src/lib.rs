//! The Cloudflare verification exchange between the sidecar, the app and the verification helper.
//!
//! The sidecar writes `request-*.txt` (a site wants a "verify you are human" check) and
//! `fetch-*.txt` (a request to make inside a verified browser window) under
//! `<data>/sidecar/cloudflare`. The watcher answers each at once: sites the viewer has not allowed
//! get "ask again", allowed sites go to the helper process (`cloudflare_window`), which opens one
//! window per site and writes the cookies and browser answers back.
#![forbid(unsafe_code)]

mod allowed_hosts;
mod exchange;
mod fetch_exchange;
mod helper_protocol;
mod verify_helper;
mod watch;

pub use allowed_hosts::AllowedHosts;
pub use exchange::{
    clear_folder, format_result, fresh_files, parse_request, pending_requests, write_named,
    write_result, Outcome, VerifyRequest,
};
pub use fetch_exchange::{
    format_fetch_result, parse_fetch_request, pending_fetches, write_fetch_result, FetchOutcome,
    FetchRequest,
};
pub use helper_protocol::{
    format_command, format_status, parse_command, parse_status, HelperCommand, HelperStatus,
};
pub use verify_helper::{HelperLaunch, VerifyHelper};
pub use watch::{spawn_watcher, CloudflareEvent, WatchConfig, WatchHandle};
