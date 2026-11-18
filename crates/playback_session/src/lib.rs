//! Finds and checks streams without a WebView shell.
//!
//! Replaces the Tauri-bound `SourceManager` open path for the egui product: start a provider
//! link search, probe candidates with [`StreamChecker`], and report the best working link.
#![forbid(unsafe_code)]

mod checked_open;

pub use checked_open::{check_and_pick, CheckedOpen, OpenError, OpenProgress};
