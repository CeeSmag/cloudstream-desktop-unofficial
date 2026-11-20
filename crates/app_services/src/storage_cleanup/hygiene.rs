//! Safe filesystem hygiene tasks for periodic and manual storage cleanup.

use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::CleanupReport;

/// Trim `logs/cloudstream.log` once it grows past 8 MiB.
pub const MAIN_LOG_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// Bytes retained from the tail of `logs/cloudstream.log` when trimmed (2 MiB).
pub const MAIN_LOG_KEEP_BYTES: u64 = 2 * 1024 * 1024;
/// Trim `cloudflare_browser` caches automatically once they exceed 150 MiB.
pub const BROWSER_CACHE_THRESHOLD_BYTES: u64 = 150 * 1024 * 1024;

/// Rotated debug logs (`*.old`) older than 3 days are deleted during periodic cleanup.
const DEBUG_OLD_MAX_AGE: Duration = Duration::from_secs(3 * 24 * 60 * 60);
/// Prefix of unpacked portable program directories under `data_dir`.
pub(crate) const RUNTIME_PREFIX: &str = "runtime-";
/// Maximum recursion depth when locating Chromium cache directories under `cloudflare_browser`.
const BROWSER_WALK_MAX_DEPTH: usize = 6;
/// Chromium cache directory names that can be cleared safely without touching `Cookies` or profile.
pub(crate) const BROWSER_CACHE_NAMES: [&str; 8] = [
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "GrShaderCache",
    "ShaderCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
];

/// Deletes older unpacked portable `runtime-*` folders inside `data_dir`, leaving `active_runtime`.
pub fn remove_old_runtimes(data_dir: &Path, active_runtime: Option<&str>) -> CleanupReport {
    let mut report = CleanupReport::default();
    let Ok(entries) = fs::read_dir(data_dir) else {
        return report;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(RUNTIME_PREFIX) || Some(name.as_str()) == active_runtime {
            continue;
        }
        let path = entry.path();
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
            report.add(remove_entry_counted(&path));
        }
    }
    report
}

/// Trims `log_file` in place when larger than `max_bytes`, keeping up to `keep_bytes` of whole
/// trailing lines so an open `append(true)` logger handle continues writing seamlessly.
pub fn trim_main_log(log_file: &Path, max_bytes: u64, keep_bytes: u64) -> CleanupReport {
    let Ok(meta) = fs::metadata(log_file) else {
        return CleanupReport::default();
    };
    let size = meta.len();
    if !meta.is_file() || size <= max_bytes || keep_bytes >= max_bytes {
        return CleanupReport::default();
    }
    let Some(tail) = read_tail_lines(log_file, size, keep_bytes) else {
        return CleanupReport::default();
    };
    if rewrite_in_place(log_file, &tail).is_err() {
        return CleanupReport::default();
    }
    CleanupReport {
        files: 0,
        bytes: size.saturating_sub(tail.len() as u64),
    }
}

fn read_tail_lines(log_file: &Path, size: u64, keep_bytes: u64) -> Option<Vec<u8>> {
    let mut file = File::open(log_file).ok()?;
    let start = size.saturating_sub(keep_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    if start == 0 {
        return Some(buffer);
    }
    let kept = buffer
        .iter()
        .position(|byte| *byte == b'\n')
        .and_then(|pos| buffer.get(pos.saturating_add(1)..))
        .unwrap_or(&buffer);
    Some(kept.to_vec())
}

fn rewrite_in_place(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(bytes)?;
    file.set_len(bytes.len() as u64)?;
    file.flush()
}

/// Cleans rotated `*.old` files in `debug_dir`, and in manual (`full`) mode also deletes closed
/// debug log files when `debug_enabled` is false.
pub fn clean_debug_logs(
    debug_dir: &Path,
    debug_enabled: bool,
    full: bool,
    now: SystemTime,
) -> CleanupReport {
    let mut report = CleanupReport::default();
    let Ok(entries) = fs::read_dir(debug_dir) else {
        return report;
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
        let removable = if full {
            is_old || !debug_enabled
        } else {
            is_old && is_older_than(&meta, now, DEBUG_OLD_MAX_AGE)
        };
        if removable {
            report.add(remove_entry_counted(&path));
        }
    }
    report
}

/// Deletes stale exchange files or directories inside `<data>/sidecar/cloudflare/`.
pub fn clean_stale_cloudflare(
    cloudflare_dir: &Path,
    max_age: Duration,
    now: SystemTime,
) -> CleanupReport {
    let mut report = CleanupReport::default();
    let Ok(entries) = fs::read_dir(cloudflare_dir) else {
        return report;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_older_than(&meta, now, max_age) {
            report.add(remove_entry_counted(&path));
        }
    }
    report
}

/// Deletes orphaned `*.cs3.part` plugin downloads older than `max_age`. Never touches `.cs3` files.
pub fn clean_stale_extension_parts(
    ext_dir: &Path,
    max_age: Duration,
    now: SystemTime,
) -> CleanupReport {
    let mut report = CleanupReport::default();
    let Ok(entries) = fs::read_dir(ext_dir) else {
        return report;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_part = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".cs3.part"));
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_part && meta.is_file() && is_older_than(&meta, now, max_age) {
            report.add(remove_entry_counted(&path));
        }
    }
    report
}

/// Clears Chromium cache folders under `browser_dir` while preserving cookies and profile state.
/// When `threshold_bytes` is `Some(limit)`, skips removal unless total cache size exceeds `limit`.
pub fn clean_browser_caches(browser_dir: &Path, threshold_bytes: Option<u64>) -> CleanupReport {
    let mut cache_dirs = Vec::new();
    collect_browser_cache_dirs(browser_dir, 0, &mut cache_dirs);
    if let Some(limit) = threshold_bytes {
        let total: u64 = cache_dirs.iter().map(|dir| dir_stats(dir).1).sum();
        if total <= limit {
            return CleanupReport::default();
        }
    }
    let mut report = CleanupReport::default();
    for dir in cache_dirs {
        report.add(remove_entry_counted(&dir));
    }
    report
}

/// Locates Chromium cache subdirectories without crossing symlinks or entering matched cache dirs.
pub(crate) fn collect_browser_cache_dirs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth >= BROWSER_WALK_MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        let is_cache = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| BROWSER_CACHE_NAMES.contains(&name));
        if is_cache {
            out.push(path);
        } else {
            collect_browser_cache_dirs(&path, depth + 1, out);
        }
    }
}

/// Returns `(file_count, total_bytes)` under `path`, never following symlinks.
pub(crate) fn dir_stats(path: &Path) -> (u64, u64) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return (0, 0);
    };
    if meta.is_file() {
        return (1, meta.len());
    }
    if !meta.is_dir() {
        return (0, 0);
    }
    let Ok(entries) = fs::read_dir(path) else {
        return (0, 0);
    };
    entries.flatten().fold((0u64, 0u64), |acc, entry| {
        let (files, bytes) = dir_stats(&entry.path());
        (acc.0.saturating_add(files), acc.1.saturating_add(bytes))
    })
}

/// Removes a file or directory (never following symlinks) and reports removed `(files, bytes)`.
pub(crate) fn remove_entry_counted(path: &Path) -> CleanupReport {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return CleanupReport::default();
    };
    if meta.is_file() {
        let bytes = meta.len();
        return match fs::remove_file(path) {
            Ok(()) => CleanupReport { files: 1, bytes },
            Err(error) => {
                tracing::debug!(path = %path.display(), %error, "keeping file in use");
                CleanupReport::default()
            }
        };
    }
    if !meta.is_dir() {
        return CleanupReport::default();
    }
    let (files, bytes) = dir_stats(path);
    match fs::remove_dir_all(path) {
        Ok(()) => CleanupReport { files, bytes },
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "keeping directory in use");
            CleanupReport::default()
        }
    }
}

pub(crate) fn is_older_than(meta: &Metadata, now: SystemTime, max_age: Duration) -> bool {
    meta.modified()
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age > max_age)
}
