use std::fs;
use std::time::Duration;

use super::*;
use crate::test_folder::{cached, many, title, Scratch, Time};

const KEY: &str = "provider\nmovie-1";
const MINUTE: Duration = Duration::from_secs(60);

fn rig() -> (Scratch, Time, TitleCache) {
    let scratch = Scratch::new();
    let time = Time::new();
    let cache = scratch.cache(CacheLimits::default(), &time);
    (scratch, time, cache)
}

fn hit(lookup: Lookup) -> CacheHit {
    match lookup {
        Lookup::Hit(hit) => hit,
        Lookup::Miss(miss) => panic!("expected saved sources, got {miss:?}"),
    }
}

#[test]
fn what_is_saved_comes_back_with_its_health_and_last_good() {
    let (_scratch, _time, cache) = rig();
    let mut saved = title(vec![
        cached("a", 720, LinkHealth::Working),
        cached("b", 1080, LinkHealth::Unchecked),
        cached("c", 480, LinkHealth::Broken),
    ]);
    saved.last_good = Some("a".to_owned());
    saved.complete = false;

    let report = cache.save(KEY, &saved).expect("saved");
    let found = hit(cache.load(KEY));

    assert_eq!(report.links, 3);
    assert!(report.bytes > 0);
    assert!(!found.title.complete);
    assert_eq!(found.title.last_good.as_deref(), Some("a"));
    let urls: Vec<_> = found
        .title
        .links
        .iter()
        .map(|c| c.link.url.as_str())
        .collect();
    assert_eq!(urls, ["a", "b", "c"]);
    assert_eq!(found.title.links[2].health, LinkHealth::Broken);
}

#[test]
fn a_title_never_saved_is_a_miss() {
    let (_scratch, _time, cache) = rig();
    assert_eq!(cache.load(KEY), Lookup::Miss(Miss::NotSaved));
}

#[test]
fn different_titles_have_different_files_and_the_same_title_the_same_one() {
    let (scratch, _time, cache) = rig();
    cache.save("p\nmovie-1", &many("a", 2)).expect("saved");
    cache.save("p\nmovie-2", &many("b", 3)).expect("saved");
    cache.save("p\nmovie-1", &many("c", 4)).expect("saved");
    assert_eq!(scratch.files().len(), 2);
    assert_eq!(hit(cache.load("p\nmovie-1")).title.links.len(), 4);
    assert_eq!(hit(cache.load("p\nmovie-2")).title.links.len(), 3);
}

#[test]
fn just_saved_is_fresh_and_twenty_minutes_later_it_is_stale() {
    let (_scratch, time, cache) = rig();
    cache.save(KEY, &many("a", 1)).expect("saved");
    time.advance(19 * MINUTE + Duration::from_secs(59));
    assert_eq!(hit(cache.load(KEY)).freshness, Freshness::Fresh);
    time.advance(Duration::from_secs(1));
    let stale = hit(cache.load(KEY));
    assert_eq!(stale.freshness, Freshness::Stale);
    assert_eq!(stale.age, Some(20 * MINUTE));
}

#[test]
fn after_a_day_the_title_is_a_miss_and_its_file_is_removed() {
    let (scratch, time, cache) = rig();
    cache.save(KEY, &many("a", 1)).expect("saved");
    time.advance(Duration::from_secs(24 * 3600));
    assert_eq!(hit(cache.load(KEY)).freshness, Freshness::Stale);
    time.advance(Duration::from_secs(1));
    assert_eq!(cache.load(KEY), Lookup::Miss(Miss::Expired));
    assert!(scratch.files().is_empty(), "{:?}", scratch.files());
}

#[test]
fn a_clock_set_backwards_makes_the_title_stale_with_an_unknown_age() {
    let (_scratch, time, cache) = rig();
    cache.save(KEY, &many("a", 1)).expect("saved");
    time.set(time.now() - 600);
    let found = hit(cache.load(KEY));
    assert_eq!(found.freshness, Freshness::Stale);
    assert_eq!(found.age, None);
}

#[test]
fn saving_cuts_to_the_best_200_links_and_keeps_each_address_once() {
    let (_scratch, _time, cache) = rig();
    let mut saved = many("a", 300);
    saved.links.push(saved.links[299].clone());
    let report = cache.save(KEY, &saved).expect("saved");
    assert_eq!(report.links, 200);
    let found = hit(cache.load(KEY));
    assert_eq!(found.title.links.len(), 200);
    assert_eq!(found.title.links[0].link.quality, 299, "best quality first");
    assert_eq!(
        found.title.links[199].link.quality, 100,
        "the worst 100 were cut"
    );
}

#[test]
fn a_file_over_the_size_limit_is_refused_and_the_old_one_stays() {
    let scratch = Scratch::new();
    let time = Time::new();
    let limits = CacheLimits {
        max_file_bytes: 2_000,
        ..CacheLimits::default()
    };
    let cache = scratch.cache(limits, &time);
    cache.save(KEY, &many("small", 2)).expect("small fits");
    let before = scratch.files();

    let refused = cache.save(KEY, &many("big", 60));

    assert!(matches!(
        refused,
        Err(TitleCacheError::TooLarge { limit: 2_000, .. })
    ));
    assert_eq!(
        scratch.files(),
        before,
        "nothing new on disk, no temporary file"
    );
    assert_eq!(hit(cache.load(KEY)).title.links.len(), 2);
}

#[test]
fn a_temporary_file_left_by_a_crash_is_ignored_when_loading() {
    let (scratch, _time, cache) = rig();
    cache.save("other", &many("a", 1)).expect("saved");
    let key_file = cache.path_of(KEY);
    let leftover = key_file.with_extension("json.tmp99-0");
    fs::write(&leftover, b"{\"version\":1,\"saved_at\":1,\"lin").expect("written");
    assert_eq!(cache.load(KEY), Lookup::Miss(Miss::NotSaved));
    assert!(leftover.exists(), "loading must not touch other files");
    assert_eq!(scratch.files().len(), 2);
}

fn load_with_file(content: &[u8]) -> (Lookup, bool) {
    let (_scratch, _time, cache) = rig();
    cache
        .save("seed", &many("seed", 1))
        .expect("makes the folder");
    let path = cache.path_of(KEY);
    fs::write(&path, content).expect("written");
    let lookup = cache.load(KEY);
    (lookup, path.exists())
}

#[test]
fn an_empty_truncated_or_garbage_file_is_a_miss_and_is_deleted() {
    let good = br#"{"version":2,"saved_at":1700000000,"complete":true,"links":[]}"#;
    for (content, label) in [
        (&b""[..], "empty"),
        (&good[..30], "truncated"),
        (&b"not json at all"[..], "garbage"),
        (&b"[1,2,3]"[..], "wrong shape"),
        (&b"{\"version\":2}"[..], "missing saved_at"),
    ] {
        let (lookup, still_there) = load_with_file(content);
        assert_eq!(lookup, Lookup::Miss(Miss::Corrupt), "{label}");
        assert!(!still_there, "{label}: the file should be deleted");
    }
}

#[test]
fn a_file_of_another_format_version_is_a_miss_and_is_deleted() {
    let other = br#"{"version":1,"saved_at":1700000000,"something":"old"}"#;
    let (lookup, still_there) = load_with_file(other);
    assert_eq!(lookup, Lookup::Miss(Miss::WrongVersion(1)));
    assert!(!still_there);
}

#[test]
fn a_file_over_the_size_limit_on_disk_is_a_miss_and_is_deleted() {
    let huge = vec![b' '; 1024 * 1024 + 1];
    let (lookup, still_there) = load_with_file(&huge);
    assert_eq!(lookup, Lookup::Miss(Miss::TooLarge));
    assert!(!still_there);
}

#[test]
fn a_field_this_version_does_not_know_is_ignored() {
    let (scratch, _time, cache) = rig();
    cache
        .save("seed", &many("seed", 1))
        .expect("makes the folder");
    let json = format!(
        r#"{{"version":2,"saved_at":{},"complete":true,"future":[1,2],"links":[{{"link":{{"url":"u","extra":1}},"health":"working","note":"x"}}],"last_good":"u"}}"#,
        crate::test_folder::START
    );
    fs::write(cache.path_of(KEY), json).expect("written");
    let found = hit(cache.load(KEY));
    assert_eq!(found.title.links[0].link.url, "u");
    assert_eq!(found.title.last_good.as_deref(), Some("u"));
    drop(scratch);
}

#[test]
fn saving_where_the_folder_cannot_be_made_fails_with_a_disk_error() {
    let scratch = Scratch::new();
    fs::write(&scratch.0, b"a file where the folder should be").expect("written");
    let cache = scratch.cache(CacheLimits::default(), &Time::new());

    let error = cache.save(KEY, &many("a", 1)).expect_err("cannot save");

    assert!(
        matches!(
            error,
            TitleCacheError::Disk {
                action: "create",
                ..
            }
        ),
        "{error}"
    );
    assert!(error.to_string().contains("cannot create"), "{error}");
    assert!(matches!(cache.load(KEY), Lookup::Miss(_)));
    assert!(fs::remove_file(&scratch.0).is_ok());
}

#[test]
fn noting_a_link_does_not_make_old_links_look_fresh() {
    let (_scratch, time, cache) = rig();
    cache.save(KEY, &many("a", 3)).expect("saved");
    time.advance(19 * MINUTE);
    assert!(cache
        .mark_broken(KEY, "https://h.test/a-2")
        .expect("updated"));
    time.advance(2 * MINUTE);
    assert_eq!(hit(cache.load(KEY)).freshness, Freshness::Stale);
}

#[test]
fn a_broken_link_moves_last_and_a_link_that_played_well_moves_first() {
    let (_scratch, _time, cache) = rig();
    cache.save(KEY, &many("a", 3)).expect("saved");
    cache
        .mark_broken(KEY, "https://h.test/a-2")
        .expect("updated");
    cache
        .mark_played_well(KEY, "https://h.test/a-0")
        .expect("updated");
    let urls: Vec<_> = hit(cache.load(KEY))
        .title
        .links
        .iter()
        .map(|c| c.link.url.clone())
        .collect();
    assert_eq!(
        urls,
        [
            "https://h.test/a-0",
            "https://h.test/a-1",
            "https://h.test/a-2"
        ]
    );
}

#[test]
fn noting_a_link_of_a_title_that_is_not_saved_does_nothing() {
    let (scratch, _time, cache) = rig();
    assert!(!cache.mark_broken(KEY, "u").expect("no error"));
    assert!(scratch.files().is_empty());
}

#[test]
fn forgetting_removes_the_file_and_says_whether_there_was_one() {
    let (scratch, _time, cache) = rig();
    cache.save(KEY, &many("a", 1)).expect("saved");
    assert!(cache.forget(KEY).expect("removed"));
    assert!(!cache.forget(KEY).expect("nothing to remove"));
    assert_eq!(cache.load(KEY), Lookup::Miss(Miss::NotSaved));
    assert!(scratch.files().is_empty());
}
