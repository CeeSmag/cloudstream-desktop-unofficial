//! The tasks of the cleanup queue (`cleanup_queue`) that tidy up files nobody needs any more. Each one
//! only touches things that can be made again, and logs what it deleted and what it could not.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::storage_cap_plan::Kind;
use crate::storage_cap_scan::{collect, delete};

/// The main log is trimmed when it is bigger than this, down to `LOG_KEEP_BYTES` of its newest lines.
pub const LOG_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const LOG_KEEP_BYTES: u64 = 2 * 1024 * 1024;
/// Exchange files with the sidecar older than this are leftovers (a normal exchange takes seconds).
pub const EXCHANGE_MAX_AGE: Duration = Duration::from_secs(60 * 60);
/// Rotated debug logs (`*.old`) older than this are deleted.
pub const OLD_LOG_MAX_AGE: Duration = Duration::from_secs(3 * 24 * 3600);
/// The check browser's cache is deleted when it is bigger than this.
pub const BROWSER_CACHE_MAX_BYTES: u64 = 150 * 1024 * 1024;

/// Deletes the program folders (`runtime-*`) of older versions; the running one stays.
pub fn old_program_folders(data_dir: &Path, running: Option<&Path>) {
    let old: Vec<_> = collect(data_dir, running)
        .into_iter()
        .filter(|entry| entry.kind == Kind::OldRuntime)
        .collect();
    delete_all("older program folder", &old);
}

/// Deletes the check browser's cache folders when together they are over `limit` bytes.
pub fn browser_cache(data_dir: &Path, limit: u64) {
    let caches: Vec<_> = collect(data_dir, None)
        .into_iter()
        .filter(|entry| entry.kind == Kind::BrowserCache)
        .collect();
    let total: u64 = caches.iter().map(|entry| entry.bytes).sum();
    if total <= limit {
        tracing::info!(
            mb = total >> 20,
            limit_mb = limit >> 20,
            "the check browser's cache is small enough; kept"
        );
        return;
    }
    delete_all("browser cache", &caches);
}

fn delete_all(what: &str, entries: &[crate::storage_cap_plan::Entry]) {
    if entries.is_empty() {
        tracing::info!(what, "nothing to delete");
    }
    for entry in entries {
        match delete(entry) {
            Ok(()) => {
                tracing::info!(what, path = %entry.path.display(), mb = entry.bytes >> 20, "deleted")
            }
            Err(error) => {
                tracing::warn!(what, path = %entry.path.display(), %error, "cannot delete it (in use?); it stays")
            }
        }
    }
}

/// Deletes files in `dir` that were last changed more than `max_age` ago and whose name satisfies `wanted`.
pub fn old_files(dir: &Path, max_age: Duration, wanted: impl Fn(&str) -> bool) {
    let Ok(listing) = fs::read_dir(dir) else {
        tracing::debug!(dir = %dir.display(), "no such folder to tidy");
        return;
    };
    let mut deleted = 0;
    for entry in listing.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let age = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok());
        if !entry.path().is_file() || !wanted(&name) || age.is_none_or(|age| age < max_age) {
            continue;
        }
        match fs::remove_file(entry.path()) {
            Ok(()) => deleted += 1,
            Err(error) => {
                tracing::warn!(file = %name, %error, "cannot delete an old file; it stays")
            }
        }
    }
    tracing::info!(dir = %dir.display(), deleted, "old files tidied");
}

/// Cuts `path` down to its newest whole lines (about `keep` bytes) when it is bigger than `max`.
/// The app keeps appending to the file meanwhile; only a line written in the very moment of the cut
/// can be lost. Returns the sizes (before, after), or `None` when nothing had to be done.
pub fn trim_log(path: &Path, max: u64, keep: u64) -> io::Result<Option<(u64, u64)>> {
    let len = fs::metadata(path)?.len();
    if len <= max {
        return Ok(None);
    }
    let mut reader = fs::File::open(path)?;
    reader.seek(SeekFrom::Start(len.saturating_sub(keep)))?;
    let mut tail = Vec::new();
    reader.read_to_end(&mut tail)?;
    // Start at a whole line: drop the partial one the cut landed in.
    let start = tail.iter().position(|&b| b == b'\n').map_or(0, |at| at + 1);
    let kept = tail.get(start..).unwrap_or_default();
    let mut writer = OpenOptions::new().write(true).open(path)?;
    writer.set_len(0)?;
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(kept)?;
    Ok(Some((len, kept.len() as u64)))
}

/// [`trim_log`] with the result logged (after the cut, so the line lands in the trimmed file).
pub fn trim_main_log(path: &Path) {
    match trim_log(path, LOG_MAX_BYTES, LOG_KEEP_BYTES) {
        Ok(Some((before, after))) => {
            tracing::info!(
                before_mb = before >> 20,
                after_mb = after >> 20,
                "the main log was cut to its newest lines"
            );
        }
        Ok(None) => tracing::info!("the main log is small enough"),
        Err(error) => tracing::warn!(%error, path = %path.display(), "cannot trim the main log"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cs-tasks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn a_small_log_is_left_alone() {
        let dir = scratch("small");
        let log = dir.join("a.log");
        fs::write(&log, "line\n").expect("write");
        assert_eq!(trim_log(&log, 1000, 100).expect("ok"), None);
        assert_eq!(fs::read_to_string(&log).expect("read"), "line\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_big_log_keeps_its_newest_whole_lines() {
        let dir = scratch("big");
        let log = dir.join("a.log");
        let text: String = (0..100).map(|n| format!("line number {n:03}\n")).collect();
        fs::write(&log, &text).expect("write");
        let (before, after) = trim_log(&log, 500, 200).expect("ok").expect("trimmed");
        let now = fs::read_to_string(&log).expect("read");
        assert_eq!(before, text.len() as u64);
        assert_eq!(after, now.len() as u64);
        assert!(now.len() <= 200, "kept {} bytes", now.len());
        assert!(now.starts_with("line number "), "starts mid-line: {now:?}");
        assert!(now.ends_with("line number 099\n"), "lost the newest line");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn appending_still_works_after_the_cut() {
        let dir = scratch("append");
        let log = dir.join("a.log");
        let text: String = (0..100).map(|n| format!("line number {n:03}\n")).collect();
        fs::write(&log, &text).expect("write");
        let mut app = OpenOptions::new()
            .append(true)
            .open(&log)
            .expect("appender");
        trim_log(&log, 500, 200).expect("ok");
        app.write_all(b"after the cut\n").expect("append");
        let now = fs::read_to_string(&log).expect("read");
        assert!(now.ends_with("line number 099\nafter the cut\n"), "{now:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_old_matching_files_are_deleted() {
        let dir = scratch("old");
        fs::write(dir.join("keep.txt"), "x").expect("write");
        fs::write(dir.join("fetch-1.txt"), "x").expect("write");
        // Nothing is an hour old yet, so nothing goes; with a zero age the matching file goes.
        old_files(&dir, Duration::from_secs(3600), |n| n.starts_with("fetch-"));
        assert!(dir.join("fetch-1.txt").exists());
        old_files(&dir, Duration::ZERO, |n| n.starts_with("fetch-"));
        assert!(!dir.join("fetch-1.txt").exists());
        assert!(
            dir.join("keep.txt").exists(),
            "a file that does not match must stay"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn older_program_folders_go_and_the_running_one_stays() {
        let dir = scratch("runtime");
        for name in ["runtime-0.3.6-a", "runtime-0.3.7-b"] {
            fs::create_dir_all(dir.join(name)).expect("dir");
            fs::write(dir.join(name).join("x.bin"), "x").expect("file");
        }
        fs::write(dir.join("cloudstream.db"), "db").expect("db");
        old_program_folders(&dir, Some(&dir.join("runtime-0.3.7-b")));
        assert!(!dir.join("runtime-0.3.6-a").exists());
        assert!(dir.join("runtime-0.3.7-b").exists());
        assert!(dir.join("cloudstream.db").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_small_browser_cache_is_kept_and_a_big_one_goes_but_not_the_cookies() {
        let dir = scratch("browser");
        let cache = dir.join("cloudflare_browser/EBWebView/Default/Cache");
        fs::create_dir_all(&cache).expect("dir");
        fs::write(cache.join("data_0"), vec![1u8; 5000]).expect("cache");
        let cookies = dir.join("cloudflare_browser/EBWebView/Default/Cookies");
        fs::write(&cookies, "c").expect("cookies");
        browser_cache(&dir, 10_000);
        assert!(cache.exists(), "a cache under the limit must stay");
        browser_cache(&dir, 1_000);
        assert!(!cache.exists());
        assert!(cookies.exists(), "cookies must never be deleted");
        let _ = fs::remove_dir_all(&dir);
    }
}
