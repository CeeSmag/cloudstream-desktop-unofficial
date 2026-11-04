//! After the viewer switches provider, what was saved before is not used and is cleaned away.

use std::time::Duration;

use super::*;
use crate::test_folder::{many, Scratch, Time};

const KEY: &str = "provider\nmovie";
const MINUTE: Duration = Duration::from_secs(60);

fn rig() -> (Scratch, Time, TitleCache) {
    let scratch = Scratch::new();
    let time = Time::new();
    let cache = scratch.cache(CacheLimits::default(), &time);
    (scratch, time, cache)
}

#[test]
fn what_was_saved_before_the_switch_is_a_miss_and_its_file_is_removed() {
    let (scratch, time, cache) = rig();
    cache.save(KEY, &many("a", 3)).expect("saved");
    time.advance(MINUTE);
    cache.invalidate_now();
    assert_eq!(cache.load(KEY), Lookup::Miss(Miss::Invalidated));
    assert!(scratch.files().is_empty(), "{:?}", scratch.files());
}

#[test]
fn what_is_saved_after_the_switch_is_used_again_and_the_switch_second_itself_counts() {
    let (_scratch, time, cache) = rig();
    time.advance(MINUTE);
    cache.invalidate_now();
    cache
        .save(KEY, &many("a", 2))
        .expect("saved in the same second");
    assert!(matches!(cache.load(KEY), Lookup::Hit(_)));
    time.advance(MINUTE);
    cache.save("other", &many("b", 1)).expect("saved later");
    assert!(matches!(cache.load("other"), Lookup::Hit(_)));
}

#[test]
fn a_save_from_the_second_before_the_switch_is_not_used() {
    let (_scratch, time, cache) = rig();
    cache.save(KEY, &many("a", 1)).expect("saved");
    time.advance(Duration::from_secs(1));
    cache.invalidate_now();
    assert_eq!(cache.load(KEY), Lookup::Miss(Miss::Invalidated));
}

#[test]
fn notes_about_links_do_not_bring_an_invalidated_title_back() {
    let (scratch, time, cache) = rig();
    cache.save(KEY, &many("a", 2)).expect("saved");
    time.advance(MINUTE);
    cache.invalidate_now();
    assert!(!cache
        .mark_broken(KEY, "https://h.test/a-0")
        .expect("no error"));
    assert!(!cache
        .mark_played_well(KEY, "https://h.test/a-1")
        .expect("no error"));
    assert_eq!(
        scratch.files().len(),
        1,
        "left for the cleanup, not rewritten"
    );
}

#[test]
fn the_cleanup_removes_what_was_saved_before_the_switch_and_keeps_the_rest() {
    let (scratch, time, cache) = rig();
    cache.save("old-1", &many("a", 1)).expect("saved");
    cache.save("old-2", &many("b", 1)).expect("saved");
    time.advance(MINUTE);
    cache.invalidate_now();
    cache.save("new", &many("c", 1)).expect("saved");
    let report = cache.prune();
    assert_eq!(
        report.invalidated, 0,
        "the save already cleaned up: {report:?}"
    );
    assert_eq!(scratch.files().len(), 1);
    assert!(matches!(cache.load("new"), Lookup::Hit(_)));
}

#[test]
fn the_cleanup_counts_the_invalidated_files_it_removes() {
    let (scratch, time, cache) = rig();
    cache.save("old-1", &many("a", 1)).expect("saved");
    cache.save("old-2", &many("b", 1)).expect("saved");
    time.advance(MINUTE);
    cache.invalidate_now();
    let report = cache.prune();
    assert_eq!((report.invalidated, report.kept), (2, 0), "{report:?}");
    assert!(scratch.files().is_empty());
}

#[test]
fn the_time_of_the_last_switch_is_restored_at_start() {
    let (_scratch, time, cache) = rig();
    let saved_at = time.now();
    cache.save(KEY, &many("a", 1)).expect("saved");
    cache.set_valid_from(saved_at + 100);
    assert_eq!(cache.load(KEY), Lookup::Miss(Miss::Invalidated));
    time.advance(Duration::from_secs(200));
    cache.save(KEY, &many("b", 1)).expect("saved");
    assert!(matches!(cache.load(KEY), Lookup::Hit(_)));
}

#[test]
fn an_earlier_switch_time_never_brings_back_what_a_later_one_dropped() {
    let (_scratch, time, cache) = rig();
    let saved_at = time.now();
    cache.save(KEY, &many("a", 1)).expect("saved");
    cache.set_valid_from(saved_at + 100);
    cache.set_valid_from(1);
    cache
        .save("older", &many("b", 1))
        .expect("saved before the later time");
    assert!(matches!(cache.load("older"), Lookup::Miss(_)));
    assert!(matches!(cache.load(KEY), Lookup::Miss(_)));
}
