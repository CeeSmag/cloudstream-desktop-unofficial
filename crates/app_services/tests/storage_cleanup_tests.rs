//! Unit and integration tests for filesystem hygiene, storage-cap eviction, and cleanup runner.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use app_services::{
    clean_browser_caches, clean_debug_logs, clean_stale_cloudflare, clean_stale_extension_parts,
    enforce_storage_cap, plan_evictions, remove_old_runtimes, trim_main_log, CapScanOptions,
    CleanupDependencies, CleanupMode, CleanupTask, EvictionKind, RemovableItem,
    StorageCleanupRunner,
};
use subtitle_store::SubtitleCache;
use title_cache::{CacheLimits, TitleCache};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> std::io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cs_storage_cleanup_{label}_{}_{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}

#[test]
fn remove_old_runtimes_deletes_older_folders_and_keeps_active() {
    let dir = TempDir::new("runtimes").expect("temp dir");
    write_file(&dir.path().join("runtime-0.4.6/app.exe"), &[1u8; 50]).expect("old 1");
    write_file(&dir.path().join("runtime-0.4.7/app.exe"), &[2u8; 30]).expect("old 2");
    write_file(&dir.path().join("runtime-0.4.8/app.exe"), &[3u8; 40]).expect("active");
    write_file(&dir.path().join("cloudstream.db"), b"db").expect("db");

    let report = remove_old_runtimes(dir.path(), Some("runtime-0.4.8"));
    assert_eq!(report.files, 2);
    assert_eq!(report.bytes, 80);
    assert!(!dir.path().join("runtime-0.4.6").exists());
    assert!(!dir.path().join("runtime-0.4.7").exists());
    assert!(dir.path().join("runtime-0.4.8/app.exe").is_file());
    assert!(dir.path().join("cloudstream.db").is_file());
}

#[test]
fn trim_main_log_keeps_whole_trailing_lines_and_preserves_open_append_handle() {
    let dir = TempDir::new("trim_log").expect("temp dir");
    let log = dir.path().join("logs/cloudstream.log");
    fs::create_dir_all(dir.path().join("logs")).expect("logs dir");

    let mut append_handle = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .expect("open append");
    for i in 0..40 {
        writeln!(append_handle, "line-{i:03}-padding-0123456789").expect("write line");
    }
    append_handle.flush().expect("flush");
    let before = fs::metadata(&log).expect("meta").len();
    assert!(before > 600);

    let report = trim_main_log(&log, 400, 200);
    assert!(report.bytes > 0);

    let trimmed = fs::read_to_string(&log).expect("read trimmed");
    assert!(trimmed.len() <= 200);
    assert!(
        trimmed.starts_with("line-"),
        "should start on a full line boundary, got: {trimmed:?}"
    );
    assert!(trimmed.contains("line-039"));

    // Existing append(true) handle must keep writing cleanly at the new end of file.
    writeln!(append_handle, "after-trim-line").expect("append after trim");
    append_handle.flush().expect("flush after trim");
    let after = fs::read_to_string(&log).expect("read after");
    assert!(after.ends_with("after-trim-line\n"));
}

#[test]
fn clean_debug_logs_respects_open_state_and_periodic_vs_manual() {
    let dir = TempDir::new("debug_logs").expect("temp dir");
    let debug = dir.path().join("logs/debug");
    write_file(&debug.join("player.log"), b"active").expect("active");
    write_file(&debug.join("player.log.old"), b"rotated").expect("rotated");
    let now = SystemTime::now();

    // Periodic run right now keeps recent *.old (< 3 days) and active log.
    let report = clean_debug_logs(&debug, true, false, now);
    assert_eq!(report.files, 0);
    assert!(debug.join("player.log.old").is_file());

    // Periodic run 4 days later removes *.old, still keeps active log.
    let four_days_later = now + Duration::from_secs(4 * 24 * 60 * 60);
    let report = clean_debug_logs(&debug, true, false, four_days_later);
    assert_eq!(report.files, 1);
    assert!(!debug.join("player.log.old").exists());
    assert!(debug.join("player.log").is_file());

    // Manual run while debug logging is enabled keeps active log, deletes new *.old.
    write_file(&debug.join("catalog.log.old"), b"rotated2").expect("rotated2");
    let report = clean_debug_logs(&debug, true, true, now);
    assert_eq!(report.files, 1);
    assert!(debug.join("player.log").is_file());

    // Manual run when debug logging is off deletes closed debug log too.
    let report = clean_debug_logs(&debug, false, true, now);
    assert_eq!(report.files, 1);
    assert!(!debug.join("player.log").exists());
}

#[test]
fn clean_stale_cloudflare_and_extension_parts_respect_age_and_protect_cs3() {
    let dir = TempDir::new("stale_temp").expect("temp dir");
    let cf = dir.path().join("sidecar/cloudflare");
    let ext = dir.path().join("extensions");
    write_file(&cf.join("req-1.json"), b"req").expect("cf req");
    write_file(&ext.join("plugin.cs3"), b"installed").expect("cs3");
    write_file(&ext.join("broken.cs3.part"), b"partial").expect("part");

    let now = SystemTime::now();
    let max_age = Duration::from_secs(3600);
    assert_eq!(clean_stale_cloudflare(&cf, max_age, now).files, 0);
    assert_eq!(clean_stale_extension_parts(&ext, max_age, now).files, 0);

    let two_hours_later = now + Duration::from_secs(7200);
    assert_eq!(
        clean_stale_cloudflare(&cf, max_age, two_hours_later).files,
        1
    );
    assert_eq!(
        clean_stale_extension_parts(&ext, max_age, two_hours_later).files,
        1
    );
    assert!(!cf.join("req-1.json").exists());
    assert!(!ext.join("broken.cs3.part").exists());
    assert!(ext.join("plugin.cs3").is_file());
}

#[test]
fn clean_browser_caches_preserves_cookies_and_respects_threshold() {
    let dir = TempDir::new("browser").expect("temp dir");
    let profile = dir.path().join("cloudflare_browser/Default");
    write_file(&profile.join("Cookies"), b"keep-me").expect("cookies");
    write_file(&profile.join("Cache/data_0"), &[0u8; 100]).expect("cache");
    write_file(&profile.join("Code Cache/js/chunk"), &[0u8; 80]).expect("code cache");
    write_file(&profile.join("GPUCache/index"), &[0u8; 20]).expect("gpu cache");

    // Under threshold -> untouched.
    let under = clean_browser_caches(&dir.path().join("cloudflare_browser"), Some(500));
    assert_eq!(under.files, 0);
    assert!(profile.join("Cache/data_0").is_file());

    // Over threshold -> caches removed, Cookies preserved.
    let over = clean_browser_caches(&dir.path().join("cloudflare_browser"), Some(150));
    assert_eq!(over.files, 3);
    assert_eq!(over.bytes, 200);
    assert!(!profile.join("Cache").exists());
    assert!(!profile.join("Code Cache").exists());
    assert!(!profile.join("GPUCache").exists());
    assert!(profile.join("Cookies").is_file());
}

fn candidate(path: &str, kind: EvictionKind, bytes: u64, secs: u64) -> RemovableItem {
    RemovableItem {
        path: PathBuf::from(path),
        kind,
        bytes,
        modified: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
        is_dir: matches!(kind, EvictionKind::OldRuntime | EvictionKind::BrowserCache),
    }
}

#[test]
fn plan_evictions_orders_cheapest_then_oldest_including_artwork() {
    let items = vec![
        candidate("sources/a.json", EvictionKind::Sources, 100, 100),
        candidate("artwork/newer.img", EvictionKind::Artwork, 100, 200),
        candidate("artwork/older.img", EvictionKind::Artwork, 100, 100),
        candidate("runtime-old", EvictionKind::OldRuntime, 100, 200),
    ];

    let planned = plan_evictions(500, 250, items);
    let paths: Vec<_> = planned.iter().map(|item| item.path.clone()).collect();
    assert_eq!(
        paths,
        vec![
            PathBuf::from("runtime-old"),
            PathBuf::from("artwork/older.img"),
            PathBuf::from("artwork/newer.img"),
        ]
    );
}

#[test]
fn enforce_storage_cap_evicts_only_enough_and_protects_db_and_extensions() {
    let dir = TempDir::new("cap").expect("temp dir");
    write_file(&dir.path().join("cloudstream.db"), &[0u8; 100]).expect("db");
    write_file(&dir.path().join("extensions/p.cs3"), &[0u8; 100]).expect("plugin");
    write_file(&dir.path().join("artwork/poster.img"), &[0u8; 300]).expect("poster");
    write_file(&dir.path().join("sources/s.json"), &[0u8; 300]).expect("source");

    let options = CapScanOptions {
        data_dir: dir.path(),
        active_runtime: None,
        debug_enabled: false,
        now: SystemTime::now(),
    };
    // Total is 800 bytes; cap at 550 should remove artwork (300) and leave sources (300), db, plugin.
    let report = enforce_storage_cap(&options, 550);
    assert_eq!(report.files, 1);
    assert_eq!(report.bytes, 300);
    assert!(!dir.path().join("artwork/poster.img").exists());
    assert!(dir.path().join("sources/s.json").is_file());
    assert!(dir.path().join("cloudstream.db").is_file());
    assert!(dir.path().join("extensions/p.cs3").is_file());
}

fn seed_runner_fixture(root: &Path) -> std::io::Result<()> {
    write_file(&root.join("cloudstream.db"), b"database")?;
    write_file(&root.join("extensions/plugin.cs3"), b"plugin")?;
    write_file(&root.join("runtime-0.4.7/old.exe"), &[1u8; 64])?;
    write_file(&root.join("runtime-0.4.8/cur.exe"), &[2u8; 64])?;
    write_file(&root.join("artwork/fresh.img"), &[3u8; 50])?;
    write_file(&root.join("subtitles/fresh.vtt"), b"WEBVTT\n")?;
    write_file(&root.join("cloudflare_browser/Default/Cookies"), b"cookie")?;
    write_file(
        &root.join("cloudflare_browser/Default/Cache/item"),
        &[4u8; 40],
    )
}

fn build_runner(root: &Path, stale_updates: Arc<AtomicUsize>) -> StorageCleanupRunner {
    StorageCleanupRunner::new(CleanupDependencies {
        data_dir: root.to_path_buf(),
        active_runtime: Some("runtime-0.4.8".into()),
        title_cache: Arc::new(TitleCache::new(
            root.join("sources"),
            CacheLimits::default(),
        )),
        subtitles: Arc::new(SubtitleCache::new(
            root.join("subtitles"),
            Duration::from_secs(7 * 24 * 60 * 60),
        )),
        artwork_dir: root.join("artwork"),
        storage_cap_bytes: Arc::new(|| 1024 * 1024 * 1024),
        is_debug_logging_enabled: Arc::new(|| false),
        is_playback_busy: Arc::new(|| false),
        clean_stale_updates: Arc::new(move |_| {
            stale_updates.fetch_add(1, Ordering::SeqCst);
        }),
    })
}

#[test]
fn runner_periodic_removes_old_runtime_and_keeps_fresh_caches() {
    let dir = TempDir::new("runner_periodic").expect("temp dir");
    seed_runner_fixture(dir.path()).expect("seed");
    let stale_updates = Arc::new(AtomicUsize::new(0));
    let runner = build_runner(dir.path(), Arc::clone(&stale_updates));

    let periodic = runner.run(CleanupMode::Periodic);
    assert!(periodic.bytes >= 64);
    assert_eq!(stale_updates.load(Ordering::SeqCst), 1);
    assert!(!dir.path().join("runtime-0.4.7").exists());
    assert!(dir.path().join("runtime-0.4.8/cur.exe").is_file());
    assert!(dir.path().join("artwork/fresh.img").is_file());
    assert!(dir.path().join("subtitles/fresh.vtt").is_file());
    assert!(dir
        .path()
        .join("cloudflare_browser/Default/Cache/item")
        .is_file());
}

#[test]
fn runner_manual_clears_regenerable_and_browser_cache_while_protecting_core_files() {
    let dir = TempDir::new("runner_manual").expect("temp dir");
    seed_runner_fixture(dir.path()).expect("seed");
    let runner = build_runner(dir.path(), Arc::new(AtomicUsize::new(0)));

    let manual = runner.run(CleanupMode::Manual);
    assert!(manual.files >= 4);
    assert!(!dir.path().join("runtime-0.4.7").exists());
    assert!(!dir.path().join("artwork/fresh.img").exists());
    assert!(!dir.path().join("subtitles/fresh.vtt").exists());
    assert!(!dir.path().join("cloudflare_browser/Default/Cache").exists());
    assert!(dir
        .path()
        .join("cloudflare_browser/Default/Cookies")
        .is_file());
    assert!(dir.path().join("cloudstream.db").is_file());
    assert!(dir.path().join("extensions/plugin.cs3").is_file());
    assert!(dir.path().join("runtime-0.4.8/cur.exe").is_file());
}
