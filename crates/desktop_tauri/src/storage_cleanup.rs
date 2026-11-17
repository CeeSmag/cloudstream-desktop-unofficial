//! One place that tidies regenerable data folder contents: used by Clean up now, home-after-ready,
//! and (piecewise) by the hourly cleanup queue.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::State;
use title_cache::TitleCache;

use crate::app_state::AppState;
use crate::cleanup_handle::CleanupHandle;
use crate::cleanup_tasks::{
    browser_cache, old_files, old_program_folders, trim_main_log, BROWSER_CACHE_MAX_BYTES,
    OLD_LOG_MAX_AGE,
};
use crate::data_commands::DataFolder;
use crate::source_manager::SourceManager;
use crate::storage_cap::{self, running_folder, StorageDto};
use crate::update_folder::cleanup_stale_updates;

/// What an on-demand cleanup needs besides the data folder path.
pub(crate) struct CleanupParts {
    pub log_file: PathBuf,
    pub sources: Arc<TitleCache>,
    pub subtitles: Arc<subtitle_store::SubtitleLoader>,
    pub cap_gb: u32,
}

/// Runs every safe mid-session tidy, enforces the storage limit, and resets the hourly schedule.
pub(crate) fn run_all(data_dir: &Path, parts: &CleanupParts, handle: &CleanupHandle) -> StorageDto {
    tracing::info!("running an on-demand storage cleanup");
    let running = running_folder(data_dir);
    old_program_folders(data_dir, running.as_deref());
    cleanup_stale_updates(data_dir);
    let report = parts.sources.prune();
    tracing::info!(?report, "saved stream sources pruned");
    let removed = parts.subtitles.cache().prune();
    tracing::info!(removed, "old subtitle files removed");
    trim_main_log(&parts.log_file);
    old_files(
        &data_dir.join("logs").join("debug"),
        OLD_LOG_MAX_AGE,
        |name| name.ends_with(".old"),
    );
    browser_cache(data_dir, BROWSER_CACHE_MAX_BYTES);
    storage_cap::enforce(data_dir, parts.cap_gb, running.as_deref());
    handle.reset_schedule();
    storage_cap::describe(data_dir, parts.cap_gb)
}

/// Clean up now / home-after-ready: tidies regenerable data and returns the refreshed storage DTO.
#[tauri::command]
#[tracing::instrument(level = "info", skip_all, err)]
pub async fn run_storage_cleanup(
    state: State<'_, AppState>,
    folder: State<'_, DataFolder>,
    sources: State<'_, SourceManager>,
    handle: State<'_, CleanupHandle>,
) -> Result<StorageDto, String> {
    let cap_gb = state
        .read()
        .settings
        .storage_cap_gb()
        .map_err(|e| e.to_string())?;
    let data_dir = folder.0.clone();
    let parts = CleanupParts {
        log_file: data_dir.join("logs").join("cloudstream.log"),
        sources: Arc::clone(&sources.titles),
        subtitles: Arc::clone(&sources.subtitles),
        cap_gb,
    };
    let handle = handle.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_all(&data_dir, &parts, &handle))
        .await
        .map_err(|error| format!("cannot run storage cleanup: {error}"))
}
