//! Finds and checks streams without a WebView shell.
//!
//! Searches provider links, probes candidates with [`StreamChecker`], plays the first working
//! stream ASAP, and keeps checking the full list (optional TitleCache fresh/stale seeds).
#![forbid(unsafe_code)]

mod checked_open;
mod open_run;
mod open_types;
mod session;

pub use checked_open::{check_and_pick, open_with};
pub use open_types::{
    save_open, title_key, CheckedOpen, OpenCache, OpenError, OpenHooks, OpenProgress,
};
pub use session::{
    refresh_sources, run_open, LiveSources, OpenControl, OpenRequest, RefreshRequest,
};
