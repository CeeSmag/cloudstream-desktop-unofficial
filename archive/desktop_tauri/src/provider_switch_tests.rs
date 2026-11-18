use std::sync::Arc;
use std::time::Duration;

use storage::{Database, SettingsStore};
use title_cache::{CachedTitle, Lookup};

use super::*;
use crate::test_titles::TestTitles;

fn settings() -> SettingsService {
    SettingsService::new(SettingsStore::new(Arc::new(
        Database::open_in_memory().expect("memory db"),
    )))
}

fn saved(titles: &TestTitles, key: &str) {
    titles
        .cache
        .save(key, &CachedTitle::default())
        .expect("saved");
}

#[test]
fn switching_to_another_provider_drops_every_saved_source_and_remembers_when() {
    let titles = TestTitles::new();
    let settings = settings();
    saved(&titles, "CineStream\nmovie-1");
    saved(&titles, "Other\nshow-2");
    titles.pass(Duration::from_secs(60));

    assert!(apply_switch(
        &settings,
        &titles.cache,
        "CineStream",
        "Other"
    ));

    assert!(matches!(
        titles.cache.load("CineStream\nmovie-1"),
        Lookup::Miss(_)
    ));
    assert!(matches!(
        titles.cache.load("Other\nshow-2"),
        Lookup::Miss(_)
    ));
    assert_eq!(settings.sources_valid_from().expect("read"), 1_700_000_060);
    assert_eq!(
        std::fs::read_dir(titles.folder()).map_or(0, Iterator::count),
        0
    );
}

#[test]
fn what_is_saved_after_the_switch_is_used_normally() {
    let titles = TestTitles::new();
    let settings = settings();
    titles.pass(Duration::from_secs(60));
    apply_switch(&settings, &titles.cache, "A", "B");
    saved(&titles, "B\nmovie");
    assert!(matches!(titles.cache.load("B\nmovie"), Lookup::Hit(_)));
}

#[test]
fn choosing_the_same_provider_or_having_none_is_not_a_switch() {
    let titles = TestTitles::new();
    let settings = settings();
    saved(&titles, "A\nmovie");
    titles.pass(Duration::from_secs(60));
    for (from, to) in [("A", "A"), ("", "A"), ("A", "")] {
        assert!(
            !apply_switch(&settings, &titles.cache, from, to),
            "{from:?} -> {to:?}"
        );
    }
    assert!(matches!(titles.cache.load("A\nmovie"), Lookup::Hit(_)));
    assert_eq!(settings.sources_valid_from().expect("read"), 0);
}

#[test]
fn switching_back_drops_what_was_saved_in_between_too() {
    let titles = TestTitles::new();
    let settings = settings();
    titles.pass(Duration::from_secs(10));
    apply_switch(&settings, &titles.cache, "A", "B");
    saved(&titles, "B\nmovie");
    titles.pass(Duration::from_secs(10));
    apply_switch(&settings, &titles.cache, "B", "A");
    assert!(matches!(titles.cache.load("B\nmovie"), Lookup::Miss(_)));
}
