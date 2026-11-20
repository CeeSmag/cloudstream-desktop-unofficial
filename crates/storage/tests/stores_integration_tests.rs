//! Integration and concurrency tests for all `storage` SQLite stores.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::too_many_lines)]

use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use core_model::{Bookmark, InstalledExtension, RepositoryEntry, ResumePointer, WatchProgress};
use storage::{
    BookmarkStore, Database, InstalledExtensionStore, RepositoryStore, ResumeStore, SettingsStore,
    WatchProgressStore,
};

fn temp_db(tag: &str) -> (Arc<Database>, std::path::PathBuf) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cs_store_{tag}_{}_{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db = Arc::new(Database::open(&dir.join("cloudstream.db")).expect("open db"));
    (db, dir)
}

#[test]
fn settings_repo_and_extension_stores_round_trip() {
    let (db, dir) = temp_db("crud_meta");

    let settings = SettingsStore::new(Arc::clone(&db));
    assert_eq!(settings.get("theme").expect("get"), None);
    settings.set("theme", "dark").expect("set");
    assert_eq!(settings.get("theme").expect("get"), Some("dark".into()));

    let repos = RepositoryStore::new(Arc::clone(&db));
    repos
        .add(&RepositoryEntry {
            url: "https://repo.example/repo.json".into(),
            name: "Main".into(),
            added_at_ms: 1_000,
            icon_url: None,
        })
        .expect("add repo");
    repos
        .set_icon(
            "https://repo.example/repo.json",
            Some("https://repo.example/icon.png"),
        )
        .expect("set icon");
    let listed = repos.list().expect("list repos");
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].icon_url.as_deref(),
        Some("https://repo.example/icon.png")
    );
    repos
        .remove("https://repo.example/repo.json")
        .expect("remove");
    assert!(repos.list().expect("empty").is_empty());

    let exts = InstalledExtensionStore::new(Arc::clone(&db));
    exts.upsert(&InstalledExtension {
        file_name: "demo.cs3".into(),
        internal_name: "Demo".into(),
        repository_url: Some("https://repo.example/repo.json".into()),
        version: 3,
        trusted: false,
        icon_url: None,
        installed_at_ms: 2_000,
    })
    .expect("upsert");
    exts.set_icon_if_missing("Demo", "https://repo.example/p.png")
        .expect("set icon");
    assert_eq!(exts.list().expect("list").len(), 1);
    exts.remove("demo.cs3").expect("remove ext");
    assert!(exts.list().expect("empty").is_empty());

    drop(db);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bookmarks_resume_and_progress_stores_round_trip() {
    let (db, dir) = temp_db("crud_watch");

    let bookmarks = BookmarkStore::new(Arc::clone(&db));
    bookmarks
        .add(&Bookmark {
            provider: "Demo".into(),
            url: "https://site.example/m1".into(),
            name: "Movie 1".into(),
            poster_url: None,
            media_type: Some("Movie".into()),
            added_at_ms: 3_000,
        })
        .expect("add bm");
    assert!(bookmarks
        .contains("Demo", "https://site.example/m1")
        .expect("contains"));
    assert_eq!(bookmarks.list().expect("list bm").len(), 1);
    bookmarks
        .remove("Demo", "https://site.example/m1")
        .expect("remove bm");
    assert!(!bookmarks
        .contains("Demo", "https://site.example/m1")
        .expect("gone"));

    let resume = ResumeStore::new(Arc::clone(&db));
    resume
        .save(&ResumePointer {
            provider: "Demo".into(),
            detail_url: "https://site.example/s1".into(),
            media_key: "ep1".into(),
            title: "Show 1".into(),
            poster_url: None,
            season: Some(1),
            episode: Some(1),
            updated_at_ms: 4_000,
        })
        .expect("save resume");
    assert!(resume
        .get("Demo", "https://site.example/s1")
        .expect("get resume")
        .is_some());
    assert_eq!(resume.recent(10).expect("recent").len(), 1);
    assert_eq!(resume.all().expect("all").len(), 1);
    resume.clear().expect("clear");
    assert!(resume.all().expect("empty").is_empty());

    let progress = WatchProgressStore::new(Arc::clone(&db));
    progress
        .save(&WatchProgress {
            provider: "Demo".into(),
            media_key: "ep1".into(),
            title: "Show 1".into(),
            detail_url: "https://site.example/s1".into(),
            poster_url: None,
            position_ms: 12_000,
            duration_ms: 60_000,
            updated_at_ms: 5_000,
            season: Some(1),
            episode: Some(1),
        })
        .expect("save progress");
    assert_eq!(progress.all_media_keys().expect("keys").len(), 1);
    progress
        .remove_for_show("Demo", "https://site.example/s1")
        .expect("remove_for_show");
    assert!(progress.recent(10).expect("empty").is_empty());

    drop(db);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn database_concurrent_reads_and_writes_stay_consistent() {
    let (db, dir) = temp_db("race");
    let mut handles = Vec::new();
    for t in 0..6usize {
        let db_c = Arc::clone(&db);
        handles.push(thread::spawn(move || {
            let settings = SettingsStore::new(Arc::clone(&db_c));
            let bookmarks = BookmarkStore::new(db_c);
            for i in 0..60usize {
                let key = format!("k_{t}_{i}");
                settings.set(&key, "ok").expect("set");
                assert_eq!(settings.get(&key).expect("get").as_deref(), Some("ok"));
                let url = format!("https://example.invalid/{t}/{i}");
                bookmarks
                    .add(&Bookmark {
                        provider: "Demo".into(),
                        url: url.clone(),
                        name: format!("Title {t}-{i}"),
                        poster_url: None,
                        media_type: None,
                        added_at_ms: i as i64,
                    })
                    .expect("add");
                assert!(bookmarks.contains("Demo", &url).expect("contains"));
            }
        }));
    }
    for h in handles {
        h.join().expect("thread finished");
    }
    assert_eq!(
        BookmarkStore::new(Arc::clone(&db))
            .list()
            .expect("list")
            .len(),
        360
    );
    drop(db);
    let _ = std::fs::remove_dir_all(dir);
}
