use std::time::{Duration, Instant};

use super::*;
use crate::test_titles::TestTitles;
use title_cache::{CachedTitle, Lookup};

fn some_title() -> CachedTitle {
    CachedTitle::default()
}

/// Waits up to two seconds for `done` to become true.
fn eventually(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    done()
}

#[test]
fn the_background_cleanup_removes_what_became_unusable_while_the_app_stayed_open() {
    let titles = TestTitles::new();
    titles.cache.save("old", &some_title()).expect("saved");
    titles.pass(Duration::from_secs(60));
    titles.cache.invalidate_now();
    let folder_files = || std::fs::read_dir(titles.folder()).map_or(0, Iterator::count);
    spawn_cleanup(&titles.cache, Duration::from_millis(20));
    assert!(eventually(|| folder_files() == 0), "the cleanup never ran");
}

#[test]
fn the_cleanup_keeps_running_and_catches_what_expires_later() {
    let titles = TestTitles::new();
    spawn_cleanup(&titles.cache, Duration::from_millis(20));
    // Let the first round pass over the empty folder; only a later round can catch what follows.
    std::thread::sleep(Duration::from_millis(150));
    titles.cache.save("soon-old", &some_title()).expect("saved");
    titles.pass(Duration::from_secs(25 * 3600));
    let folder_files = || std::fs::read_dir(titles.folder()).map_or(0, Iterator::count);
    assert!(
        eventually(|| folder_files() == 0),
        "the second round never ran"
    );
    assert!(matches!(titles.cache.load("soon-old"), Lookup::Miss(_)));
}

#[test]
fn the_time_of_the_last_switch_is_applied_when_the_folder_is_opened() {
    let folder = std::env::temp_dir().join(format!("cs_open_cache_{}", std::process::id()));
    let cache = open_title_cache(&folder, u64::MAX / 2);
    cache.save("x", &some_title()).expect("saved");
    assert!(
        matches!(cache.load("x"), Lookup::Miss(_)),
        "a save from before the switch time is not used"
    );
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn the_cleanup_thread_ends_when_the_cache_is_dropped() {
    let titles = TestTitles::new();
    let weak = Arc::downgrade(&titles.cache);
    spawn_cleanup(&titles.cache, Duration::from_millis(10));
    let folder = titles.folder().to_path_buf();
    drop(titles);
    assert!(
        eventually(|| weak.upgrade().is_none()),
        "the thread still holds the cache"
    );
    std::fs::remove_dir_all(folder).ok();
}
