//! Keeping the app's data folder (`%LOCALAPPDATA%\CloudStreamRs`) under the size the viewer chose in
//! Settings > General (1 to 10 GB, 2 by default).
//!
//! When the folder is over the limit, things that can be made again are deleted, cheapest first (older
//! program folders, finished debug logs, subtitles, saved stream sources, the check browser's cache), only
//! as much as needed. The database, plugins and settings are never touched, so a limit below what the app
//! itself needs cannot be reached; that is reported, not hidden. It runs a little after start, every hour,
//! and at once when the limit is changed. Every deletion and every failure is logged.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::data_commands::DataFolder;
use crate::storage_cap_plan::{plan, Plan};
use crate::storage_cap_scan::{collect, delete, dir_size};

const BYTES_PER_GB: u64 = 1024 * 1024 * 1024;
/// The first check waits, so it does not compete with start-up.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

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

/// What the page shows: the limit, the size now, and whether it is over.
#[derive(Debug, Clone, Serialize)]
pub struct StorageDto {
    cap_gb: u32,
    used_bytes: u64,
    over: bool,
}

fn describe(data_dir: &Path, cap_gb: u32) -> StorageDto {
    let used_bytes = dir_size(data_dir);
    StorageDto {
        cap_gb,
        used_bytes,
        over: used_bytes > u64::from(cap_gb) * BYTES_PER_GB,
    }
}

fn read_cap(state: &AppState) -> Result<u32, String> {
    state
        .read()
        .settings
        .storage_cap_gb()
        .map_err(|e| e.to_string())
}

/// The limit and the size of the data folder (measured off the window thread).
#[tauri::command]
pub async fn get_storage_cap(
    state: State<'_, AppState>,
    folder: State<'_, DataFolder>,
) -> Result<StorageDto, String> {
    let cap_gb = read_cap(&state)?;
    let data_dir = folder.0.clone();
    tauri::async_runtime::spawn_blocking(move || describe(&data_dir, cap_gb))
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

/// Starts the hourly check for the data folder `data_dir`.
pub fn start(app: &AppHandle, data_dir: PathBuf) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("storage-cap".to_owned())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_AFTER);
            loop {
                check_once(&app, &data_dir);
                std::thread::sleep(CHECK_EVERY);
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start the data folder size check; the limit will only apply when it is changed");
    }
}

fn check_once(app: &AppHandle, data_dir: &Path) {
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
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cs-enforce-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn write(dir: &Path, relative: &str, bytes: usize) {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, vec![1u8; bytes]).expect("file");
    }

    #[test]
    fn a_folder_within_its_limit_is_left_alone() {
        let dir = scratch("within");
        write(&dir, "runtime-old/x.bin", 1000);
        let report = enforce(&dir, 1, None).expect("ran");
        assert_eq!((report.deleted, report.still_over), (0, false));
        assert!(dir.join("runtime-old/x.bin").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_saved_data_is_never_deleted_even_when_it_alone_is_over() {
        let dir = scratch("never");
        // 2 MB of saved data with a limit of 1 GB is within; use the plan directly for "over".
        write(&dir, "cloudstream.db", 2_000_000);
        write(&dir, "extensions/P.cs3", 1_000_000);
        write(&dir, "runtime-0.3.6/old.bin", 500_000);
        let before = dir_size(&dir);
        let chosen = plan(collect(&dir, None), before, 1_000);
        let tally = apply(&dir, &chosen);
        assert_eq!(tally, (1, 0));
        assert!(dir.join("cloudstream.db").exists());
        assert!(dir.join("extensions/P.cs3").exists());
        assert!(!dir.join("runtime-0.3.6").exists());
        assert!(chosen.still_over(1_000));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_enforcements_at_once_do_not_fail_or_double_delete() {
        let dir = scratch("race");
        for n in 0..20 {
            write(&dir, &format!("subtitles/{n}.vtt"), 100);
        }
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    let chosen = {
                        let _guard = BUSY.lock().unwrap_or_else(|p| p.into_inner());
                        let all = collect(&dir, None);
                        let total = dir_size(&dir);
                        let chosen = plan(all, total, 0);
                        apply(&dir, &chosen)
                    };
                    chosen
                })
            })
            .collect();
        let failed: usize = workers
            .into_iter()
            .map(|w| w.join().expect("worker").1)
            .sum();
        assert_eq!(
            failed, 0,
            "serialised runs must not meet already-deleted files"
        );
        assert_eq!(dir_size(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_limit_is_in_binary_gigabytes() {
        assert_eq!(BYTES_PER_GB, 1_073_741_824);
    }
}
