//! Executes one periodic hygiene pass, manual storage cleanup, or storage-cap check.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use poster_artwork::DiskCache;
use subtitle_store::SubtitleCache;
use title_cache::TitleCache;

use crate::{clean_regenerable, CleanupReport};

use super::cap_scan::{enforce_storage_cap, CapScanOptions};
use super::hygiene::{
    clean_browser_caches, clean_debug_logs, clean_stale_cloudflare, clean_stale_extension_parts,
    dir_stats, remove_old_runtimes, trim_main_log, BROWSER_CACHE_THRESHOLD_BYTES,
    MAIN_LOG_KEEP_BYTES, MAIN_LOG_MAX_BYTES,
};

/// Stale threshold for orphaned Cloudflare exchange files and partial `.cs3.part` downloads.
const STALE_TEMP_AGE: Duration = Duration::from_secs(60 * 60);

/// Which kind of cleanup pass the scheduler is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupMode {
    /// Post-Home and hourly smart pruning + junk removal + storage-cap enforcement.
    Periodic,
    /// User-triggered "Clean up storage now": clears all regenerable caches + junk + cap check.
    Manual,
    /// Storage-limit enforcement only (triggered when the user changes Settings > Storage limit).
    CapOnly,
}

/// Abstract task executed by [`super::CleanupScheduler`] (real runner in production, fake in tests).
pub trait CleanupTask: Send + Sync {
    /// Executes one cleanup pass and returns the removed file and byte counts.
    fn run(&self, mode: CleanupMode) -> CleanupReport;
    /// True while a video is loading or playing so automatic passes can defer quietly.
    fn is_playback_busy(&self) -> bool;
}

/// Collaborators injected from `app_shell` into [`StorageCleanupRunner`].
pub struct CleanupDependencies {
    /// Application data folder (`<data_dir>`).
    pub data_dir: PathBuf,
    /// Active unpacked portable runtime folder (`runtime-...`) to protect, if any.
    pub active_runtime: Option<String>,
    /// Saved stream-sources cache (`sources/`).
    pub title_cache: Arc<TitleCache>,
    /// Downloaded subtitle cache (`subtitles/`).
    pub subtitles: Arc<SubtitleCache>,
    /// Poster artwork disk cache folder (`artwork/`).
    pub artwork_dir: PathBuf,
    /// Reads the user's current storage cap in bytes.
    pub storage_cap_bytes: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// Reports whether debug log files are currently open for writing.
    pub is_debug_logging_enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Reports whether video playback is actively loading or playing.
    pub is_playback_busy: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Removes stale update download attempts under `<data_dir>/updates/`.
    pub clean_stale_updates: Arc<dyn Fn(&Path) + Send + Sync>,
}

/// Real filesystem and cache cleanup runner wired in `app_shell`.
pub struct StorageCleanupRunner {
    deps: CleanupDependencies,
}

impl StorageCleanupRunner {
    /// Creates a runner over the supplied dependencies.
    pub fn new(deps: CleanupDependencies) -> Self {
        Self { deps }
    }

    fn run_periodic(&self, now: SystemTime) -> CleanupReport {
        let mut report = self.run_common_hygiene(now, false);
        report.add(self.prune_caches());
        report.add(clean_browser_caches(
            &self.deps.data_dir.join("cloudflare_browser"),
            Some(BROWSER_CACHE_THRESHOLD_BYTES),
        ));
        report.add(self.run_cap_only(now));
        tracing::info!(
            files = report.files,
            bytes = report.bytes,
            "periodic storage cleanup finished"
        );
        report
    }

    fn run_manual(&self, now: SystemTime) -> CleanupReport {
        let mut report = clean_regenerable(&self.deps.data_dir);
        report.add(self.run_common_hygiene(now, true));
        report.add(clean_browser_caches(
            &self.deps.data_dir.join("cloudflare_browser"),
            None,
        ));
        report.add(self.run_cap_only(now));
        tracing::info!(
            files = report.files,
            bytes = report.bytes,
            "manual storage cleanup finished"
        );
        report
    }

    fn run_common_hygiene(&self, now: SystemTime, full_debug: bool) -> CleanupReport {
        let mut report =
            remove_old_runtimes(&self.deps.data_dir, self.deps.active_runtime.as_deref());
        report.add(self.run_stale_updates());
        report.add(clean_stale_extension_parts(
            &self.deps.data_dir.join("extensions"),
            STALE_TEMP_AGE,
            now,
        ));
        report.add(clean_stale_cloudflare(
            &self.deps.data_dir.join("sidecar").join("cloudflare"),
            STALE_TEMP_AGE,
            now,
        ));
        report.add(trim_main_log(
            &self.deps.data_dir.join("logs").join("cloudstream.log"),
            MAIN_LOG_MAX_BYTES,
            MAIN_LOG_KEEP_BYTES,
        ));
        report.add(clean_debug_logs(
            &self.deps.data_dir.join("logs").join("debug"),
            (self.deps.is_debug_logging_enabled)(),
            full_debug,
            now,
        ));
        report
    }

    fn run_stale_updates(&self) -> CleanupReport {
        let updates_dir = self.deps.data_dir.join("updates");
        let before = dir_stats(&updates_dir);
        (self.deps.clean_stale_updates)(&self.deps.data_dir);
        let after = dir_stats(&updates_dir);
        CleanupReport {
            files: before.0.saturating_sub(after.0),
            bytes: before.1.saturating_sub(after.1),
        }
    }

    fn prune_caches(&self) -> CleanupReport {
        let mut report = CleanupReport::default();
        report.add(measure_prune(&self.deps.data_dir.join("sources"), || {
            let _ = self.deps.title_cache.prune();
        }));
        report.add(measure_prune(&self.deps.data_dir.join("subtitles"), || {
            let _ = self.deps.subtitles.prune();
        }));
        report.add(measure_prune(&self.deps.artwork_dir, || {
            let _ = DiskCache::new(&self.deps.artwork_dir).prune();
        }));
        report
    }

    fn run_cap_only(&self, now: SystemTime) -> CleanupReport {
        let cap_bytes = (self.deps.storage_cap_bytes)();
        let options = CapScanOptions {
            data_dir: &self.deps.data_dir,
            active_runtime: self.deps.active_runtime.as_deref(),
            debug_enabled: (self.deps.is_debug_logging_enabled)(),
            now,
        };
        enforce_storage_cap(&options, cap_bytes)
    }
}

impl CleanupTask for StorageCleanupRunner {
    fn run(&self, mode: CleanupMode) -> CleanupReport {
        let now = SystemTime::now();
        match mode {
            CleanupMode::Periodic => self.run_periodic(now),
            CleanupMode::Manual => self.run_manual(now),
            CleanupMode::CapOnly => self.run_cap_only(now),
        }
    }

    fn is_playback_busy(&self) -> bool {
        (self.deps.is_playback_busy)()
    }
}

fn measure_prune(dir: &Path, prune: impl FnOnce()) -> CleanupReport {
    let before = dir_stats(dir);
    prune();
    let after = dir_stats(dir);
    CleanupReport {
        files: before.0.saturating_sub(after.0),
        bytes: before.1.saturating_sub(after.1),
    }
}
