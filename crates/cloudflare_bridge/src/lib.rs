//! The Cloudflare verification exchange between the sidecar, the app and the verification helper.
//!
//! The sidecar writes `request-*.txt` (a site wants a "verify you are human" check) and
//! `fetch-*.txt` (a request to make inside a verified browser window) under
//! `<data>/sidecar/cloudflare`. The watcher asks permission for each new site and routes approved
//! requests to the helper process. Each window belongs to a request, and the helper writes scoped
//! cookies and browser answers back.
#![forbid(unsafe_code)]

mod allowed_hosts;
mod cookie_value;
mod exchange;
mod fetch_exchange;
mod helper_idle;
mod helper_protocol;
mod issued_pass;
mod pass_cookie;
mod prompt_owners;
#[cfg(test)]
mod test_logs;
mod verify_helper;
mod watch;
mod watch_event;
mod watch_expiry;
mod watch_requests;

pub use allowed_hosts::AllowedHosts;
pub use cookie_value::CookieValue;
pub use exchange::{
    clear_folder, format_result, fresh_files, parse_request, pending_requests, write_named,
    write_result, Outcome, VerifyRequest,
};
pub use fetch_exchange::{
    format_fetch_result, parse_fetch_request, pending_fetches, write_fetch_result, FetchOutcome,
    FetchRequest,
};
pub use helper_protocol::{
    classify_status_line, format_command, format_status, parse_command, parse_status,
    HelperCommand, HelperStatus, StatusLine,
};
pub use issued_pass::{decode_pass, IssuedPass, PassWireError};
pub use pass_cookie::{
    CookieDomain, CookieExpiry, CookieParts, CookieRefusal, PassCookie, CLEARANCE_COOKIE,
};
pub use verify_helper::{HelperLaunch, VerifyHelper};
pub use watch::{spawn_watcher, WatchConfig, WatchHandle};
pub use watch_event::CloudflareEvent;
