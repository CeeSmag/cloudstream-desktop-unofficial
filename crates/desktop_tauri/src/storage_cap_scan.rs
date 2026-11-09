//! Looking at the data folder: its size, and the entries that may be deleted when it is too big
//! (see `storage_cap_plan`). Nothing outside the data folder is looked at, and the database, the
//! installed plugins, the settings and the running program are never listed.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::storage_cap_plan::{Entry, Kind};

/// A debug log changed more recently than this is still being written, so it stays.
const LOG_IN_USE_SECS: u64 = 120;
/// The browser keeps its cache in folders with these names; cookies and logins are elsewhere.
const BROWSER_CACHE_DIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "GrShaderCache",
    "ShaderCache",
];
/// How deep inside the browser's folder its caches are looked for.
const BROWSER_DEPTH: usize = 5;

/// The size in bytes of everything under `path` (unreadable things count as nothing, with a log line).
pub fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    match fs::read_dir(path) {
        Ok(children) => children
            .flatten()
            .map(|child| dir_size(&child.path()))
            .sum(),
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "cannot read a folder while measuring the data folder");
            0
        }
    }
}

/// Seconds since `path` was last changed (0 when unknown).
fn age_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map_or(0, |age| age.as_secs())
}

fn entry(path: PathBuf, kind: Kind) -> Entry {
    Entry {
        bytes: dir_size(&path),
        age_secs: age_secs(&path),
        path,
        kind,
    }
}

fn children(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|list| list.flatten().map(|child| child.path()).collect())
        .unwrap_or_default()
}

/// Every entry of `data_dir` that may be deleted. `running` is the program folder in use (never listed).
pub fn collect(data_dir: &Path, running: Option<&Path>) -> Vec<Entry> {
    let mut entries = Vec::new();
    for path in children(data_dir) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() && name.starts_with("runtime-") && Some(path.as_path()) != running {
            entries.push(entry(path, Kind::OldRuntime));
        }
    }
    for path in children(&data_dir.join("logs").join("debug")) {
        if path.is_file() && age_secs(&path) >= LOG_IN_USE_SECS {
            entries.push(entry(path, Kind::DebugLog));
        }
    }
    for path in children(&data_dir.join("subtitles")) {
        entries.push(entry(path, Kind::Subtitle));
    }
    for path in children(&data_dir.join("sources")) {
        entries.push(entry(path, Kind::Sources));
    }
    browser_caches(&data_dir.join("cloudflare_browser"), 0, &mut entries);
    entries
}

fn browser_caches(dir: &Path, depth: usize, entries: &mut Vec<Entry>) {
    if depth > BROWSER_DEPTH {
        return;
    }
    for path in children(dir).into_iter().filter(|p| p.is_dir()) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if BROWSER_CACHE_DIRS.contains(&name) {
            entries.push(entry(path, Kind::BrowserCache));
        } else {
            browser_caches(&path, depth + 1, entries);
        }
    }
}

/// Deletes one entry (a file or a whole folder).
pub fn delete(entry: &Entry) -> std::io::Result<()> {
    if entry.path.is_dir() {
        fs::remove_dir_all(&entry.path)
    } else {
        fs::remove_file(&entry.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("cs-cap-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("scratch");
            Self(dir)
        }

        fn file(&self, relative: &str, bytes: usize) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            fs::write(path, vec![7u8; bytes]).expect("file");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_size_adds_up_every_file_below() {
        let scratch = Scratch::new("size");
        scratch.file("a.bin", 100);
        scratch.file("deep/er/b.bin", 250);
        assert_eq!(dir_size(&scratch.0), 350);
        assert_eq!(dir_size(&scratch.0.join("missing")), 0);
    }

    #[test]
    fn only_things_that_can_be_made_again_are_listed() {
        let scratch = Scratch::new("collect");
        scratch.file("cloudstream.db", 10);
        scratch.file("extensions/Plugin.cs3", 10);
        scratch.file("sidecar/cloudflare/request-1.txt", 10);
        scratch.file("runtime-0.3.6-aaa/desktop_tauri.exe", 100);
        scratch.file("runtime-0.3.7-bbb/desktop_tauri.exe", 100);
        scratch.file("subtitles/a.vtt", 5);
        scratch.file("sources/t.json", 5);
        scratch.file("cloudflare_browser/EBWebView/Default/Cache/data_0", 20);
        scratch.file("cloudflare_browser/EBWebView/Default/Cookies", 20);
        let running = scratch.0.join("runtime-0.3.7-bbb");
        let mut found: Vec<String> = collect(&scratch.0, Some(&running))
            .iter()
            .map(|e| {
                e.path
                    .strip_prefix(&scratch.0)
                    .unwrap_or(&e.path)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        found.sort();
        assert_eq!(
            found,
            vec![
                "cloudflare_browser/EBWebView/Default/Cache",
                "runtime-0.3.6-aaa",
                "sources/t.json",
                "subtitles/a.vtt",
            ]
        );
    }

    #[test]
    fn a_log_that_is_still_being_written_stays() {
        let scratch = Scratch::new("logs");
        scratch.file("logs/debug/app-logs.txt", 10);
        assert!(collect(&scratch.0, None).is_empty());
    }

    #[test]
    fn deleting_removes_a_file_or_a_whole_folder() {
        let scratch = Scratch::new("delete");
        scratch.file("runtime-old/x/y.bin", 10);
        scratch.file("subtitles/a.vtt", 5);
        for e in collect(&scratch.0, None) {
            delete(&e).expect("deleted");
        }
        assert!(!scratch.0.join("runtime-old").exists());
        assert!(!scratch.0.join("subtitles/a.vtt").exists());
    }
}
