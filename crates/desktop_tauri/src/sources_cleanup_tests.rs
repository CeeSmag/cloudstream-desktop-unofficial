use std::time::Duration;

use super::*;
use crate::test_titles::TestTitles;
use title_cache::{CachedTitle, Lookup};

fn some_title() -> CachedTitle {
    CachedTitle::default()
}

#[test]
fn pruning_removes_what_became_unusable_while_the_app_stayed_open() {
    let titles = TestTitles::new();
    titles.cache.save("old", &some_title()).expect("saved");
    titles.pass(Duration::from_secs(60));
    titles.cache.invalidate_now();
    let folder_files = || std::fs::read_dir(titles.folder()).map_or(0, Iterator::count);
    assert_eq!(folder_files(), 1);
    titles.cache.prune();
    assert_eq!(folder_files(), 0, "the switched-away save stayed on disk");
}

#[test]
fn pruning_catches_what_expires_later() {
    let titles = TestTitles::new();
    titles.cache.prune();
    titles.cache.save("soon-old", &some_title()).expect("saved");
    titles.pass(Duration::from_secs(25 * 3600));
    titles.cache.prune();
    let folder_files = || std::fs::read_dir(titles.folder()).map_or(0, Iterator::count);
    assert_eq!(folder_files(), 0);
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
