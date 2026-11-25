//! The Microsoft Edge WebView2 Runtime on Windows: whether it is there, and installing it with
//! Microsoft's own setup when it is not. The Cloudflare check opens its window in WebView2; without
//! the runtime it cannot open.
//!
//! The rules (what the registry values mean, which addresses may serve the setup, whose signature
//! counts) are pure functions. The PC is reached through traits ([`RuntimeProbe`], and for the
//! install `SetupFetcher`, `SignatureVerifier`, `SetupRunner`), implemented for Windows in
//! `windows/` and by fakes in the tests.
#![forbid(unsafe_code)]

mod runtime_probe;
mod runtime_status;
mod status_board;
#[cfg(windows)]
mod windows;

pub use runtime_probe::{platform_probe, pretended_facts, FixedProbe, RuntimeProbe};
pub use runtime_status::{judge, parse_version, RuntimeFacts, RuntimeStatus, MIN_WINDOWS_MAJOR};
pub use status_board::{ProbeTicket, StatusBoard};
