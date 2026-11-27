//! Finds and checks streams without a WebView shell.
//!
//! Searches provider links, probes candidates with [`app_services::StreamChecker`], waits for a
//! full search and check (or skip / time cap), then plays the **best** working stream by
//! [`app_services::link_rank`]. Fresh TitleCache seeds can still start at once; stale seeds get a
//! short probe window first.
#![forbid(unsafe_code)]

mod check_pool;
mod checked_open;
mod load_focus;
mod open_gate;
mod open_run;
mod open_seeds;
mod open_types;
mod session;
mod subtitle_budget;
mod subtitle_priority;

pub use checked_open::{check_and_pick, open_with, OpenTarget};
pub use load_focus::LoadFocus;
pub use open_types::{
    cached_title_from_open, save_open_kept, title_key, CheckedOpen, OpenCache, OpenError,
    OpenHooks, OpenProgress,
};
pub use session::{
    refresh_sources, run_open, LiveSources, OpenControl, OpenRequest, RefreshRequest,
};
pub use subtitle_budget::{
    matches_subtitle_label, subtitle_rank, QueueBudget, MAX_OTHER_EXTERNAL, MAX_PER_OTHER_LANGUAGE,
    MAX_PREFERRED_EXTERNAL, MAX_RESERVE_TRACKS, PREFERRED_READY_PER_LANGUAGE,
};
pub use subtitle_priority::{CheckPriority, SUBTITLE_PRIORITY_GRACE};
