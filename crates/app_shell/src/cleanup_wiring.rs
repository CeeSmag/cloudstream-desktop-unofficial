//! Wires [`app_services::CleanupScheduler`] to real caches, settings, playback status, and updater cleanup.

use std::path::Path;
use std::sync::Arc;

use app_services::{
    CleanupDependencies, CleanupSchedule, CleanupScheduler, StorageCleanupRunner, UiServices,
};
use player_api::PlaybackState;

use crate::data_wipe::running_folder_name;
use crate::debug_logs::DebugLogs;

const BYTES_PER_GIGABYTE: u64 = 1024 * 1024 * 1024;
const DEFAULT_STORAGE_CAP_BYTES: u64 = 2 * BYTES_PER_GIGABYTE;

/// Starts the single background storage cleanup worker (dormant until Home is ready).
pub fn start_storage_cleanup(
    data_dir: &Path,
    services: &UiServices,
    debug_logs: &DebugLogs,
) -> CleanupScheduler {
    let exe = std::env::current_exe().ok();
    let active_runtime = running_folder_name(data_dir, exe.as_deref());
    let settings = services.settings.clone();
    let playback = services.playback.clone();
    let debug = debug_logs.clone();
    let deps = CleanupDependencies {
        data_dir: data_dir.to_path_buf(),
        active_runtime,
        title_cache: Arc::clone(&services.title_cache),
        subtitles: Arc::clone(&services.subtitles),
        artwork_dir: data_dir.join("artwork"),
        storage_cap_bytes: Arc::new(move || {
            settings
                .storage_cap_gb()
                .map(|gb| u64::from(gb) * BYTES_PER_GIGABYTE)
                .unwrap_or(DEFAULT_STORAGE_CAP_BYTES)
        }),
        is_debug_logging_enabled: Arc::new(move || debug.is_enabled()),
        is_playback_busy: Arc::new(move || {
            playback.as_ref().is_some_and(|service| {
                matches!(
                    service.status().state,
                    PlaybackState::Loading | PlaybackState::Playing
                )
            })
        }),
        clean_stale_updates: Arc::new(app_updater::remove_stale_downloads),
    };
    CleanupScheduler::start(
        Arc::new(StorageCleanupRunner::new(deps)),
        CleanupSchedule::default(),
    )
}
