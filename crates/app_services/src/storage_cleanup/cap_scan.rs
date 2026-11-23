//! Scans the data folder for removable candidates and enforces the user's storage limit.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{measure_storage, CleanupReport};

use super::cap_plan::{plan_evictions, EvictionKind, RemovableItem};
use super::hygiene::{
    collect_browser_cache_dirs, dir_stats, is_older_than, remove_entry_counted, RUNTIME_PREFIX,
};

/// Update attempts newer than 1 hour may still be downloading and are left alone.
const STALE_UPDATE_AGE: Duration = Duration::from_secs(60 * 60);

/// Inputs for scanning `data_dir` when enforcing the storage limit.
pub struct CapScanOptions<'a> {
    /// Application data directory.
    pub data_dir: &'a Path,
    /// Folder name of the currently running portable runtime (`runtime-...`), if any.
    pub active_runtime: Option<&'a str>,
    /// Whether debug log files are currently open for writing.
    pub debug_enabled: bool,
    /// Current wall clock time for age checks.
    pub now: SystemTime,
}

/// Removes the cheapest and oldest removable items when `data_dir` exceeds `cap_bytes`.
pub fn enforce_storage_cap(options: &CapScanOptions<'_>, cap_bytes: u64) -> CleanupReport {
    let total = measure_storage(options.data_dir).total_bytes;
    if total <= cap_bytes {
        return CleanupReport::default();
    }
    let items = scan_removable(options);
    let planned = plan_evictions(total, cap_bytes, items);
    let mut report = CleanupReport::default();
    for item in planned {
        report.add(remove_entry_counted(&item.path));
    }
    if report.bytes > 0 || report.files > 0 {
        tracing::info!(
            total_bytes = total,
            cap_bytes,
            removed_files = report.files,
            removed_bytes = report.bytes,
            "enforced storage cap"
        );
    }
    report
}

fn scan_removable(options: &CapScanOptions<'_>) -> Vec<RemovableItem> {
    let mut items = Vec::new();
    collect_old_runtimes(options.data_dir, options.active_runtime, &mut items);
    collect_stale_updates(&options.data_dir.join("updates"), options.now, &mut items);
    collect_debug_logs(
        &options.data_dir.join("logs").join("debug"),
        options.debug_enabled,
        &mut items,
    );
    collect_files_in(
        &options.data_dir.join("artwork"),
        EvictionKind::Artwork,
        &mut items,
    );
    collect_files_in(
        &options.data_dir.join("subtitles"),
        EvictionKind::Subtitle,
        &mut items,
    );
    collect_files_in(
        &options.data_dir.join("sources"),
        EvictionKind::Sources,
        &mut items,
    );
    collect_browser_caches(&options.data_dir.join("cloudflare_browser"), &mut items);
    items
}

fn collect_old_runtimes(
    data_dir: &Path,
    active_runtime: Option<&str>,
    out: &mut Vec<RemovableItem>,
) {
    let Ok(entries) = fs::read_dir(data_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(RUNTIME_PREFIX) || Some(name.as_str()) == active_runtime {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        let (_, bytes) = dir_stats(&path);
        out.push(RemovableItem {
            path,
            kind: EvictionKind::OldRuntime,
            bytes,
            modified: meta.modified().unwrap_or(UNIX_EPOCH),
            is_dir: true,
        });
    }
}

fn collect_stale_updates(updates_dir: &Path, now: SystemTime, out: &mut Vec<RemovableItem>) {
    let Ok(entries) = fs::read_dir(updates_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !is_older_than(&meta, now, STALE_UPDATE_AGE) {
            continue;
        }
        let (_, bytes) = dir_stats(&path);
        out.push(RemovableItem {
            path,
            kind: EvictionKind::UpdateAttempt,
            bytes,
            modified: meta.modified().unwrap_or(UNIX_EPOCH),
            is_dir: meta.is_dir(),
        });
    }
}

fn collect_debug_logs(debug_dir: &Path, debug_enabled: bool, out: &mut Vec<RemovableItem>) {
    let Ok(entries) = fs::read_dir(debug_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let is_old = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".old"));
        if debug_enabled && !is_old {
            continue;
        }
        out.push(RemovableItem {
            path,
            kind: EvictionKind::DebugLog,
            bytes: meta.len(),
            modified: meta.modified().unwrap_or(UNIX_EPOCH),
            is_dir: false,
        });
    }
}

fn collect_files_in(dir: &Path, kind: EvictionKind, out: &mut Vec<RemovableItem>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            collect_files_in(&path, kind, out);
            continue;
        }
        if meta.is_file() {
            out.push(RemovableItem {
                path,
                kind,
                bytes: meta.len(),
                modified: meta.modified().unwrap_or(UNIX_EPOCH),
                is_dir: false,
            });
        }
    }
}

fn collect_browser_caches(browser_dir: &Path, out: &mut Vec<RemovableItem>) {
    let mut dirs = Vec::new();
    collect_browser_cache_dirs(browser_dir, 0, &mut dirs);
    for path in dirs {
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let (_, bytes) = dir_stats(&path);
        out.push(RemovableItem {
            path,
            kind: EvictionKind::BrowserCache,
            bytes,
            modified: meta.modified().unwrap_or(UNIX_EPOCH),
            is_dir: true,
        });
    }
}

#[cfg(test)]
#[path = "cap_scan_tests.rs"]
mod tests;
