use std::sync::Arc;

use core_model::ResumePointer;
use storage::Database;

use super::*;

fn service() -> (LibraryService, WatchProgressStore, ResumeStore) {
    let database = Arc::new(Database::open_in_memory().expect("memory db"));
    let progress = WatchProgressStore::new(Arc::clone(&database));
    let resume = ResumeStore::new(Arc::clone(&database));
    let library = LibraryService::new(
        BookmarkStore::new(database),
        progress.clone(),
        resume.clone(),
    );
    (library, progress, resume)
}

fn pointer(show_id: &str, key: &str, ep: (i32, i32), at: i64) -> ResumePointer {
    ResumePointer {
        provider: "P".into(),
        detail_url: format!("https://site.test/{show_id}"),
        media_key: key.into(),
        title: format!("Show {show_id}"),
        poster_url: Some("poster.png".into()),
        season: Some(ep.0),
        episode: Some(ep.1),
        updated_at_ms: at,
    }
}

fn entry(key: &str, url: &str, pos: i64, dur: i64) -> WatchProgress {
    WatchProgress {
        provider: "P".into(),
        media_key: key.into(),
        title: key.into(),
        detail_url: url.into(),
        position_ms: pos,
        duration_ms: dur,
        updated_at_ms: 100,
        ..WatchProgress::default()
    }
}

fn assert_first_cw_item(item: &WatchProgress) {
    assert_eq!(item.detail_url, "https://site.test/show1");
    assert_eq!(item.media_key, "ep2");
    assert_eq!(item.season, Some(1));
    assert_eq!(item.episode, Some(2));
    assert_eq!(item.position_ms, 15_000);
    assert_eq!(item.duration_ms, 60_000);
}

fn assert_second_cw_item(item: &WatchProgress) {
    assert_eq!(item.detail_url, "https://site.test/show2");
    assert_eq!(item.position_ms, 0);
    assert_eq!(item.duration_ms, 0);
}

#[test]
fn continue_watching_returns_one_card_per_show_with_pointer_progress() {
    let (library, progress, resume) = service();
    resume
        .save(&pointer("show1", "ep2", (1, 2), 200))
        .expect("resume show1");
    resume
        .save(&pointer("show2", "ep1", (1, 1), 100))
        .expect("resume show2");
    progress
        .save(&entry("ep2", "https://site.test/show1", 15_000, 60_000))
        .expect("save ep2");

    let items = library.continue_watching().expect("continue watching");
    assert_eq!(items.len(), 2);
    assert_first_cw_item(&items[0]);
    assert_second_cw_item(&items[1]);
}

#[test]
fn resume_pointer_reads_existing_pointer_with_progress() {
    let (library, progress, resume) = service();
    let url = "https://site.test/show1";
    resume
        .save(&pointer("show1", "ep2", (1, 2), 200))
        .expect("resume");
    progress
        .save(&entry("ep2", url, 30_000, 60_000))
        .expect("save");

    let ptr = library
        .resume_pointer("P", url)
        .expect("read")
        .expect("present");
    assert_eq!(ptr.media_key, "ep2");
    assert_eq!(ptr.position_ms, 30_000);
    assert_eq!(ptr.duration_ms, 60_000);
    assert_eq!(ptr.season, Some(1));
    assert_eq!(ptr.episode, Some(2));
}

#[test]
fn remove_continue_watching_deletes_pointer_and_all_show_episodes() {
    let (library, progress, resume) = service();
    let show_url = "https://site.test/show1";
    resume
        .save(&pointer("show1", "ep2", (1, 2), 200))
        .expect("resume");
    progress
        .save(&entry("ep1", show_url, 60_000, 60_000))
        .expect("save ep1");
    progress
        .save(&entry("ep2", show_url, 10_000, 60_000))
        .expect("save ep2");

    let keys = library
        .remove_continue_watching("P", show_url)
        .expect("remove");
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"ep1".to_owned()));
    assert!(keys.contains(&"ep2".to_owned()));

    assert!(library.continue_watching().expect("cw").is_empty());
    assert!(progress.get("P", "ep1").expect("get ep1").is_none());
    assert!(progress.get("P", "ep2").expect("get ep2").is_none());
    assert!(resume.get("P", show_url).expect("get pointer").is_none());
}

#[test]
fn clear_continue_watching_clears_all_pointers_and_progress() {
    let (library, progress, resume) = service();
    resume.save(&pointer("s1", "ep1", (1, 1), 10)).expect("s1");
    resume.save(&pointer("s2", "ep1", (1, 1), 20)).expect("s2");
    progress
        .save(&entry("ep1", "https://site.test/s1", 10, 100))
        .expect("p1");

    let removed = library.clear_continue_watching().expect("clear");
    assert!(!removed.is_empty());
    assert!(library.continue_watching().expect("cw").is_empty());
    assert!(progress.all_media_keys().expect("keys").is_empty());
    assert!(resume.all().expect("all").is_empty());
}

#[test]
fn bookmark_round_trip() {
    let (service, _, _) = service();
    let bookmark = Bookmark {
        provider: "P".into(),
        url: "u".into(),
        name: "n".into(),
        ..Bookmark::default()
    };
    service.add_bookmark(&bookmark).expect("add");
    assert!(service.is_bookmarked("P", "u").expect("check"));
    service.remove_bookmark("P", "u").expect("remove");
    assert!(service.bookmarks().expect("list").is_empty());
}
