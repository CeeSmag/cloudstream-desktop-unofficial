//! What the storage limit may remove, by category: only what the app can make again, never the
//! running program, a download in progress or the debug log being written.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// A data folder of its own, removed when the test ends.
struct DataDir(PathBuf);

impl DataDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "cs_cap_scan_{tag}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        Self(dir)
    }

    fn file(&self, relative: &str, bytes: usize) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("a folder")).expect("mkdir");
        fs::write(path, vec![0u8; bytes]).expect("write");
    }

    /// What the scan lists, as `(path inside the data folder, kind, bytes)`, sorted.
    fn listed(&self, options: &CapScanOptions<'_>) -> Vec<(String, EvictionKind, u64)> {
        let mut items: Vec<_> = scan_removable(options)
            .into_iter()
            .map(|item| {
                let relative = item.path.strip_prefix(&self.0).expect("inside").to_owned();
                let relative = relative.to_string_lossy().replace('\\', "/");
                (relative, item.kind, item.bytes)
            })
            .collect();
        items.sort();
        items
    }

    fn options(&self) -> CapScanOptions<'_> {
        CapScanOptions {
            data_dir: &self.0,
            active_runtime: None,
            debug_enabled: false,
            now: SystemTime::now(),
        }
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const TWO_HOURS: Duration = Duration::from_secs(2 * 3600);

#[test]
fn older_runtimes_are_listed_whole_but_never_the_running_one_or_a_stray_file() {
    let data = DataDir::new("runtimes");
    data.file("runtime-0.4.7/cloudstream.exe", 40);
    data.file("runtime-0.4.7/lib/x.dll", 10);
    data.file("runtime-0.4.8/cloudstream.exe", 40);
    data.file("runtime-notes.txt", 5);
    let options = CapScanOptions {
        active_runtime: Some("runtime-0.4.8"),
        ..data.options()
    };
    assert_eq!(
        data.listed(&options),
        [("runtime-0.4.7".to_owned(), EvictionKind::OldRuntime, 50)]
    );
}

#[test]
fn update_attempts_are_listed_only_once_they_are_over_an_hour_old() {
    let data = DataDir::new("updates");
    data.file("updates/1-100/CloudStream.AppImage.part", 70);
    let half_an_hour = CapScanOptions {
        now: SystemTime::now() + Duration::from_secs(30 * 60),
        ..data.options()
    };
    assert!(
        data.listed(&half_an_hour).is_empty(),
        "it may still be downloading"
    );
    let later = CapScanOptions {
        now: SystemTime::now() + TWO_HOURS,
        ..data.options()
    };
    assert_eq!(
        data.listed(&later),
        [("updates/1-100".to_owned(), EvictionKind::UpdateAttempt, 70)]
    );
}

#[test]
fn debug_logs_being_written_stay_while_debug_logging_is_on() {
    let data = DataDir::new("debug");
    data.file("logs/debug/player-logs.jsonl", 30);
    data.file("logs/debug/player-logs.jsonl.old", 20);
    let on = CapScanOptions {
        debug_enabled: true,
        ..data.options()
    };
    assert_eq!(
        data.listed(&on),
        [(
            "logs/debug/player-logs.jsonl.old".to_owned(),
            EvictionKind::DebugLog,
            20
        )]
    );
    assert_eq!(
        data.listed(&data.options()).len(),
        2,
        "with logging off both may go"
    );
}

#[test]
fn cached_files_are_listed_one_by_one_from_every_level_under_their_kind() {
    let data = DataDir::new("caches");
    data.file("artwork/a.img", 3);
    data.file("subtitles/files/s.vtt", 4);
    data.file("subtitles/titles/t.jsonl", 5);
    data.file("sources/x.json", 6);
    assert_eq!(
        data.listed(&data.options()),
        [
            ("artwork/a.img".to_owned(), EvictionKind::Artwork, 3),
            ("sources/x.json".to_owned(), EvictionKind::Sources, 6),
            (
                "subtitles/files/s.vtt".to_owned(),
                EvictionKind::Subtitle,
                4
            ),
            (
                "subtitles/titles/t.jsonl".to_owned(),
                EvictionKind::Subtitle,
                5
            ),
        ]
    );
}

#[test]
fn only_the_check_browsers_cache_folders_are_listed_never_its_cookies() {
    let data = DataDir::new("browser");
    data.file("cloudflare_browser/Default/Cache/item", 40);
    data.file("cloudflare_browser/Default/Code Cache/js/x", 8);
    data.file("cloudflare_browser/Default/Cookies", 9);
    assert_eq!(
        data.listed(&data.options()),
        [
            (
                "cloudflare_browser/Default/Cache".to_owned(),
                EvictionKind::BrowserCache,
                40
            ),
            (
                "cloudflare_browser/Default/Code Cache".to_owned(),
                EvictionKind::BrowserCache,
                8
            ),
        ]
    );
}

#[test]
fn the_database_plugins_and_settings_are_never_listed() {
    let data = DataDir::new("kept");
    data.file("cloudstream.db", 100);
    data.file("extensions/p.cs3", 100);
    data.file("logs/cloudstream.log", 100);
    data.file("sidecar/state.json", 100);
    assert!(data.listed(&data.options()).is_empty());
}

#[test]
fn under_the_limit_nothing_is_removed() {
    let data = DataDir::new("under");
    data.file("artwork/a.img", 300);
    let report = enforce_storage_cap(&data.options(), 10_000);
    assert_eq!((report.files, report.bytes), (0, 0));
    assert!(data.0.join("artwork/a.img").is_file());
}
