//! Scheduled and manual storage cleanup: hygiene, cache pruning, and storage-cap enforcement.

mod cap_plan;
mod cap_scan;
mod hygiene;
mod runner;
mod scheduler;

pub use cap_plan::{plan_evictions, EvictionKind, RemovableItem};
pub use cap_scan::{enforce_storage_cap, CapScanOptions};
pub use hygiene::{
    clean_browser_caches, clean_debug_logs, clean_stale_cloudflare, clean_stale_extension_parts,
    remove_old_runtimes, trim_main_log, BROWSER_CACHE_THRESHOLD_BYTES, MAIN_LOG_KEEP_BYTES,
    MAIN_LOG_MAX_BYTES,
};
pub use runner::{CleanupDependencies, CleanupMode, CleanupTask, StorageCleanupRunner};
pub use scheduler::{CleanupHandle, CleanupSchedule, CleanupScheduler};
