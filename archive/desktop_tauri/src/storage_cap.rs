//! Keeping the app's data folder (`%LOCALAPPDATA%\CloudStreamRs`) under the size the viewer chose in
//! Settings > General (1 to 10 GB, 2 by default).
//!
//! When the folder is over the limit, things that can be made again are deleted, cheapest first (older
//! program folders, stale update downloads, finished debug logs, subtitles, saved stream sources, the check
//! browser's cache), only as much as needed. The database, plugins and settings are never touched, so a
//! limit below what the app itself needs cannot be reached; that is reported, not hidden. It runs as a
//! task of the cleanup queue (after start-up, then hourly) and at once when the limit is changed. Every
//! deletion and every failure is logged.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, TryLockError};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::data_commands::DataFolder;
use crate::storage_cap_plan::{plan, Plan};
use crate::storage_cap_scan::{collect, delete, dir_size, usage};

const BYTES_PER_GB: u64 = 1024 * 1024 * 1024;

/// Only one enforcement runs at a time (the hourly one and one asked for by a changed limit).
static BUSY: Mutex<()> = Mutex::new(());

/// What an enforcement found and did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub before: u64,
    pub after: u64,
    pub deleted: usize,
    pub failed: usize,
    pub still_over: bool,
}

/// The program folder this run was unpacked into (`runtime-*` in the data folder), which must stay.
pub fn running_folder(data_dir: &Path) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    app_shell::running_folder_name(data_dir, exe.as_deref()).map(|name| data_dir.join(name))
}

/// Takes the one place in line; `None` (logged) when another enforcement is running.
fn acquire() -> Option<MutexGuard<'static, ()>> {
    match BUSY.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => {
            tracing::info!("the data folder limit is already being applied; skipping this check");
            None
        }
    }
}

/// Brings `data_dir` down to `cap_gb` if it is over. `None` when another enforcement is running.
pub fn enforce(data_dir: &Path, cap_gb: u32, running: Option<&Path>) -> Option<Report> {
    let _guard = acquire()?;
    let cap = u64::from(cap_gb) * BYTES_PER_GB;
    let before = dir_size(data_dir);
    let chosen = plan(collect(data_dir, running), before, cap);
    if chosen.delete.is_empty() {
        log_state(before, cap, &chosen);
        return Some(Report {
            before,
            after: before,
            deleted: 0,
            failed: 0,
            still_over: chosen.still_over(cap),
        });
    }
    tracing::info!(
        used_mb = before >> 20,
        cap_gb,
        planned = chosen.delete.len(),
        "the data folder is over its limit; deleting what can be made again"
    );
    let (deleted, failed) = apply(data_dir, &chosen);
    Some(finish(data_dir, cap, (before, deleted, failed)))
}

/// Measures again after the deletions and reports (and logs) where the folder ended up.
fn finish(data_dir: &Path, cap: u64, done: (u64, usize, usize)) -> Report {
    let (before, deleted, failed) = done;
    let after = dir_size(data_dir);
    let still_over = after > cap;
    if still_over {
        tracing::warn!(used_mb = after >> 20, cap_mb = cap >> 20, "the data folder is still over its limit: what is left (database, plugins, settings, the running program) is never deleted");
    }
    tracing::info!(
        before_mb = before >> 20,
        after_mb = after >> 20,
        deleted,
        failed,
        "applied the data folder limit"
    );
    Report {
        before,
        after,
        deleted,
        failed,
        still_over,
    }
}

fn log_state(used: u64, cap: u64, chosen: &Plan) {
    if chosen.still_over(cap) {
        tracing::warn!(
            used_mb = used >> 20,
            cap_mb = cap >> 20,
            "the data folder is over its limit and nothing in it can be deleted"
        );
    } else {
        tracing::info!(
            used_mb = used >> 20,
            cap_mb = cap >> 20,
            "the data folder is within its limit"
        );
    }
}

/// Deletes the planned entries; returns (deleted, failed). Each one is logged.
fn apply(data_dir: &Path, chosen: &Plan) -> (usize, usize) {
    let (mut deleted, mut failed) = (0, 0);
    for entry in &chosen.delete {
        let shown = entry.path.strip_prefix(data_dir).unwrap_or(&entry.path);
        match delete(entry) {
            Ok(()) => {
                deleted += 1;
                tracing::info!(kind = entry.kind.label(), path = %shown.display(), mb = entry.bytes >> 20, bytes = entry.bytes, "deleted to keep the data folder within its limit");
            }
            Err(error) => {
                failed += 1;
                tracing::warn!(kind = entry.kind.label(), path = %shown.display(), %error, "cannot delete it (in use?); it stays");
            }
        }
    }
    (deleted, failed)
}

/// What the page shows: the limit, the size now, tracked folders, and whether it is over.
#[derive(Debug, Clone, Serialize)]
pub struct StorageDto {
    cap_gb: u32,
    used_bytes: u64,
    over: bool,
    logs_bytes: u64,
    subtitles_bytes: u64,
    sources_bytes: u64,
    updates_bytes: u64,
}

/// Measures the data folder for the Settings card (and for Clean up now).
pub(crate) fn describe(data_dir: &Path, cap_gb: u32) -> StorageDto {
    let used_bytes = dir_size(data_dir);
    let tracked = usage(data_dir);
    StorageDto {
        cap_gb,
        used_bytes,
        over: used_bytes > u64::from(cap_gb) * BYTES_PER_GB,
        logs_bytes: tracked.logs_bytes,
        subtitles_bytes: tracked.subtitles_bytes,
        sources_bytes: tracked.sources_bytes,
        updates_bytes: tracked.updates_bytes,
    }
}

fn read_cap(state: &AppState) -> Result<u32, String> {
    state
        .read()
        .settings
        .storage_cap_gb()
        .map_err(|e| e.to_string())
}

/// When over the limit, deletes what can be remade and asks the cleanup queue to run soon.
fn enforce_if_over(data_dir: &Path, cap_gb: u32, app: &AppHandle) {
    let used = dir_size(data_dir);
    let cap = u64::from(cap_gb) * BYTES_PER_GB;
    if used <= cap {
        return;
    }
    tracing::info!(
        used_mb = used >> 20,
        cap_gb,
        "data folder over its limit on read; enforcing"
    );
    let running = running_folder(data_dir);
    enforce(data_dir, cap_gb, running.as_deref());
    if let Some(handle) = app.try_state::<crate::cleanup_handle::CleanupHandle>() {
        handle.request_now();
    }
}

/// The limit and the size of the data folder (measured off the window thread).
/// When the folder is over the limit, deletes what can be made again before answering.
#[tauri::command]
pub async fn get_storage_cap(
    state: State<'_, AppState>,
    folder: State<'_, DataFolder>,
    app: AppHandle,
) -> Result<StorageDto, String> {
    let cap_gb = read_cap(&state)?;
    let data_dir = folder.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        enforce_if_over(&data_dir, cap_gb, &app);
        describe(&data_dir, cap_gb)
    })
    .await
    .map_err(|error| format!("cannot measure the data folder: {error}"))
}

/// Saves the new limit and applies it at once.
#[tauri::command]
#[tracing::instrument(level = "info", skip_all, fields(gb = gb), err)]
pub async fn set_storage_cap(
    gb: u32,
    state: State<'_, AppState>,
    folder: State<'_, DataFolder>,
) -> Result<StorageDto, String> {
    state
        .read()
        .settings
        .set_storage_cap_gb(gb)
        .map_err(|e| e.to_string())?;
    tracing::info!(gb, "the viewer changed the data folder limit");
    let data_dir = folder.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let running = running_folder(&data_dir);
        enforce(&data_dir, gb, running.as_deref());
        describe(&data_dir, gb)
    })
    .await
    .map_err(|error| format!("cannot apply the data folder limit: {error}"))
}

/// Applies the saved limit to `data_dir` (a task of the cleanup queue).
pub(crate) fn check_once(app: &AppHandle, data_dir: &Path) {
    let Some(state) = app.try_state::<AppState>() else {
        tracing::warn!("no settings yet; the data folder limit was not checked");
        return;
    };
    match read_cap(&state) {
        Ok(cap_gb) => {
            enforce(data_dir, cap_gb, running_folder(data_dir).as_deref());
        }
        Err(reason) => {
            tracing::warn!(%reason, "cannot read the data folder limit; it was not applied")
        }
    }
}

#[cfg(test)]
#[path = "storage_cap_tests.rs"]
mod tests;
