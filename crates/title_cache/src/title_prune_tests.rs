use std::fs;
use std::time::Duration;

use super::*;
use crate::cache_limits::CacheLimits;
use crate::stored_file::Miss;
use crate::test_folder::{many, real_now, Scratch, Time};
use crate::title_cache::Lookup;

const HOUR: Duration = Duration::from_secs(3600);

fn limited(max_titles: usize) -> CacheLimits {
    CacheLimits {
        max_titles,
        ..CacheLimits::default()
    }
}

fn age_file(path: &std::path::Path, age: Duration) {
    let file = fs::OpenOptions::new().write(true).open(path).expect("open");
    file.set_modified(real_now() - age).expect("set time");
}

#[test]
fn the_oldest_saves_go_first_when_more_titles_are_saved_than_the_limit() {
    let scratch = Scratch::new();
    let time = Time::new();
    let cache = scratch.cache(limited(3), &time);
    for n in 0..5 {
        cache.save(&format!("t{n}"), &many("a", 1)).expect("saved");
        time.advance(Duration::from_secs(10));
    }
    let kept: Vec<_> = (0..5)
        .filter(|n| matches!(cache.load(&format!("t{n}")), Lookup::Hit(_)))
        .collect();
    assert_eq!(kept, [2, 3, 4], "the newest three stay");
    assert_eq!(scratch.files().len(), 3);
}

#[test]
fn saving_prunes_the_surplus_at_once_and_says_nothing_to_the_caller() {
    let scratch = Scratch::new();
    let time = Time::new();
    let cache = scratch.cache(limited(1), &time);
    cache.save("first", &many("a", 1)).expect("saved");
    time.advance(Duration::from_secs(1));
    cache.save("second", &many("b", 1)).expect("saved");
    assert_eq!(cache.load("first"), Lookup::Miss(Miss::NotSaved));
    assert!(matches!(cache.load("second"), Lookup::Hit(_)));
}

#[test]
fn expired_and_unreadable_files_are_removed_and_good_ones_kept() {
    let scratch = Scratch::new();
    let time = Time::new();
    let cache = scratch.cache(CacheLimits::default(), &time);
    cache.save("old", &many("a", 1)).expect("saved");
    time.advance(25 * HOUR);
    cache.save("new", &many("b", 1)).expect("saved");
    let corrupt = scratch.0.join("0000.json");
    let stranger = scratch.0.join("notes.txt");
    fs::write(&corrupt, b"x").expect("written");
    fs::write(&stranger, b"x").expect("written");

    let report = cache.prune();

    assert_eq!((report.corrupt, report.kept), (1, 1));
    assert!(report.bytes > 0);
    assert!(!corrupt.exists());
    assert!(stranger.exists(), "files that are not ours are left alone");
    assert!(matches!(cache.load("new"), Lookup::Hit(_)));
}

#[test]
fn a_temporary_file_is_removed_only_once_it_is_old_enough_to_be_a_crash_leftover() {
    let scratch = Scratch::new();
    let cache = scratch.cache(CacheLimits::default(), &Time::new());
    cache.save("seed", &many("a", 1)).expect("saved");
    let leftover = scratch.0.join("1111.json.tmp5-0");
    let young = scratch.0.join("2222.json.tmp6-0");
    fs::write(&leftover, b"x").expect("written");
    fs::write(&young, b"x").expect("written");
    age_file(&leftover, Duration::from_secs(120));

    let report = cache.prune();

    assert_eq!(report.leftovers, 1);
    assert!(!leftover.exists());
    assert!(young.exists(), "a save in progress elsewhere must survive");
}

#[test]
fn the_report_counts_what_expired_and_what_was_evicted() {
    let scratch = Scratch::new();
    let time = Time::new();
    let cache = scratch.cache(limited(2), &time);
    cache.save("ancient", &many("a", 1)).expect("saved");
    time.advance(30 * HOUR);
    for n in 0..3 {
        cache.save(&format!("t{n}"), &many("a", 1)).expect("saved");
        time.advance(Duration::from_secs(5));
    }
    let report = cache.prune();
    assert_eq!(
        report,
        PruneReport {
            kept: 2,
            bytes: report.bytes,
            ..PruneReport::default()
        }
    );
    assert_eq!(scratch.files().len(), 2);
}

#[test]
fn pruning_a_folder_that_does_not_exist_is_fine() {
    let scratch = Scratch::new();
    let cache = scratch.cache(CacheLimits::default(), &Time::new());
    assert_eq!(cache.prune(), PruneReport::default());
}

#[test]
fn a_file_dated_by_a_clock_far_in_the_future_is_removed() {
    let scratch = Scratch::new();
    let time = Time::new();
    let cache = scratch.cache(CacheLimits::default(), &time);
    time.set(time.now() + 400 * 24 * 3600);
    cache.save("from-the-future", &many("a", 1)).expect("saved");
    time.set(time.now() - 400 * 24 * 3600);
    assert_eq!(cache.prune().expired, 1);
}

#[test]
fn the_report_counts_the_titles_a_lower_limit_evicts() {
    let scratch = Scratch::new();
    let time = Time::new();
    let roomy = scratch.cache(limited(10), &time);
    for n in 0..5 {
        roomy.save(&format!("t{n}"), &many("a", 1)).expect("saved");
        time.advance(Duration::from_secs(5));
    }
    // The same folder read with a lower limit (the setting was lowered): three must go.
    let report = scratch.cache(limited(2), &time).prune();
    assert_eq!((report.evicted, report.kept), (3, 2));
    assert_eq!(scratch.files().len(), 2);
}

/// A file the app cannot read (permissions, a lock) may be fine again later: prune leaves it.
#[cfg(unix)]
#[test]
fn a_saved_file_that_cannot_be_read_is_left_alone() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = Scratch::new();
    let time = Time::new();
    let cache = scratch.cache(CacheLimits::default(), &time);
    cache.save("locked", &many("a", 1)).expect("saved");
    let path = cache.path_of("locked");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod");
    assert!(
        fs::read(&path).is_err(),
        "precondition: a mode-000 file must be unreadable (run the tests as a normal user, not root)"
    );

    let report = cache.prune();

    let still_there = path.exists();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("restore");
    assert!(still_there, "an unreadable file is not deleted");
    assert_eq!((report.corrupt, report.kept), (0, 0));
}
