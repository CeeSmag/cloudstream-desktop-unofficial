//! Finds and checks streams without a WebView shell.
//!
//! Searches provider links, probes candidates with [`StreamChecker`], waits for a full search and
//! check (or skip / time cap), then plays the **best** working stream by [`link_rank`]. Fresh
//! TitleCache seeds can still start at once; stale seeds get a short probe window first.
#![forbid(unsafe_code)]

mod checked_open;
mod open_gate;
mod open_run;
mod open_seeds;
mod open_types;
mod session;

pub use checked_open::{check_and_pick, open_with, OpenTarget};
pub use open_types::{
    cached_title_from_open, save_open_kept, title_key, CheckedOpen, OpenCache, OpenError,
    OpenHooks, OpenProgress,
};
pub use session::{
    refresh_sources, run_open, LiveSources, OpenControl, OpenRequest, RefreshRequest,
};
