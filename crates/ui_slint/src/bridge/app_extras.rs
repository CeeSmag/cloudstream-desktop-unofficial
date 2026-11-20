//! Optional runtime extras injected by `app_shell` (not part of [`app_services::UiServices`]).

use std::path::PathBuf;

use app_services::CleanupHandle;

/// Asks for "delete all saved data" at the next start (built by the shell, which runs the wipe).
pub type WipeRequest = Box<dyn Fn() -> std::io::Result<()>>;

/// Switches the app's debug log files on or off (built by the shell, which owns the logger).
pub type DebugLogSwitch = Box<dyn Fn(bool) -> std::io::Result<()>>;

/// Optional runtime extras that are not part of [`app_services::UiServices`].
#[derive(Default)]
pub struct AppExtras {
    /// Sidecar Cloudflare challenge watcher.
    pub cloudflare: Option<cloudflare_bridge::WatchHandle>,
    /// Data directory for logs / updates.
    pub data_dir: Option<PathBuf>,
    /// Schedules the data wipe.
    pub request_wipe: Option<WipeRequest>,
    /// Settings > General > Debug logging.
    pub debug_logs: Option<DebugLogSwitch>,
    /// Coordinated post-Home, hourly, and manual storage cleanup scheduler.
    pub cleanup: Option<CleanupHandle>,
}
