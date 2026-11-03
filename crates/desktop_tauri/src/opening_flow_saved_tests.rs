//! Opening a video whose title was opened before: the sources saved the first time decide whether
//! the second opening waits for the search, and the search still refreshes what is saved.

use title_cache::{Freshness, Lookup};

use super::*;
use crate::video_opening::title_key;

const SEARCH_SECONDS: u64 = 3;

fn item() -> PlaybackItem {
    PlaybackItem {
        provider: "Fake".to_owned(),
        media_key: "title".to_owned(),
        ..PlaybackItem::default()
    }
}

/// A search of `count` streams, one every 100 ms, that ends `tail` after the last.
fn script(count: usize, tail: Duration) -> SearchScript {
    SearchScript::steady(count, Duration::from_millis(100), 720, tail)
}

/// Opens the title once the normal way (waiting for everything) so its sources are saved.
fn open_first_time(count: usize) -> Rig {
    let rig = rig(
        script(count, Duration::from_millis(100)),
        limits(5_000, 30_000),
    );
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    rig.sources.close();
    rig
}

fn saved(rig: &Rig) -> title_cache::CachedTitle {
    match rig.titles.cache.load(&title_key(&item())) {
        Lookup::Hit(hit) => hit.title,
        Lookup::Miss(miss) => panic!("nothing saved: {miss:?}"),
    }
}

#[test]
fn the_first_opening_saves_the_streams_when_the_search_and_checks_are_done() {
    let rig = open_first_time(3);
    let title = saved(&rig);
    assert!(title.complete);
    assert_eq!(title.links.len(), 3);
    assert!(title
        .links
        .iter()
        .all(|cached| cached.health == title_cache::LinkHealth::Working));
    let played = rig.player.played.lock().expect("lock").clone();
    assert_eq!(
        title.last_good.as_deref(),
        played.first().map(String::as_str)
    );
}

#[test]
fn a_title_opened_again_soon_plays_at_once_while_the_search_runs_and_refreshes_the_saved_list() {
    let first = open_first_time(3);
    let second = rig_sharing(
        &first,
        script(30, Duration::from_secs(SEARCH_SECONDS)),
        limits(5_000, 30_000),
    );

    let (outcome, background, waited) = open(&second);

    assert_eq!(outcome, Ok(()));
    assert!(
        waited < Duration::from_millis(1_500),
        "waited {waited:?} with fresh saved sources"
    );
    assert_eq!(second.player.played.lock().expect("lock").len(), 1);
    assert_eq!(
        second.searches.stats().active,
        1,
        "the search must still run"
    );

    join_within(background, TIME_LIMIT);
    let title = saved(&second);
    assert!(title.complete);
    assert_eq!(
        title.links.len(),
        30,
        "the fresh search replaced the saved list"
    );
    let listed = second
        .sources
        .session()
        .snapshot()
        .expect("open")
        .links
        .len();
    assert_eq!(
        listed, 30,
        "saved streams and found streams must not be listed twice"
    );
}

#[test]
fn saved_sources_that_are_a_while_old_are_tested_and_the_one_that_works_plays_before_the_search_ends(
) {
    let first = open_first_time(3);
    first.titles.pass(Duration::from_secs(30 * 60));
    let second = rig_sharing(
        &first,
        script(30, Duration::from_secs(SEARCH_SECONDS)),
        limits(5_000, 30_000),
    );
    assert!(matches!(
        second.titles.cache.load(&title_key(&item())),
        Lookup::Hit(hit) if hit.freshness == Freshness::Stale
    ));

    let (outcome, background, waited) = open(&second);

    assert_eq!(outcome, Ok(()));
    assert!(waited < Duration::from_millis(1_500), "waited {waited:?}");
    assert_eq!(second.player.played.lock().expect("lock").len(), 1);
    join_within(background, TIME_LIMIT);
}

#[test]
fn when_no_saved_source_works_any_more_the_video_waits_for_the_search_like_a_new_one() {
    let first = open_first_time(3);
    first.titles.pass(Duration::from_secs(30 * 60));
    let second = rig_sharing(
        &first,
        script(10, Duration::from_millis(200)),
        limits(5_000, 30_000),
    );
    second.dead.lock().expect("lock").extend([
        "host0.".to_owned(),
        "host1.".to_owned(),
        "host2.".to_owned(),
    ]);

    let (outcome, background, waited) = open(&second);

    assert_eq!(outcome, Ok(()));
    assert!(
        waited >= Duration::from_millis(1_000),
        "played after {waited:?}; must wait for the 10 streams of the search"
    );
    let played = second.player.played.lock().expect("lock").clone();
    assert_eq!(played.len(), 1);
    assert!(
        !played[0].contains("host0.")
            && !played[0].contains("host1.")
            && !played[0].contains("host2."),
        "{played:?}"
    );
    join_within(background, TIME_LIMIT);
    let title = saved(&second);
    assert!(title.complete);
    assert_dead_streams_are_remembered(&title);
}

/// The three dead streams of the test are in the saved list, marked broken.
fn assert_dead_streams_are_remembered(title: &title_cache::CachedTitle) {
    let dead: Vec<_> = title
        .links
        .iter()
        .filter(|c| {
            ["host0.", "host1.", "host2."]
                .iter()
                .any(|w| c.link.url.contains(w))
        })
        .collect();
    assert_eq!(dead.len(), 3, "the dead streams are remembered as dead");
    assert!(dead
        .iter()
        .all(|c| c.health == title_cache::LinkHealth::Broken));
}

#[test]
fn when_the_provider_fails_the_saved_sources_still_play_and_nothing_is_lost() {
    let first = open_first_time(3);
    let mut down = script(0, Duration::from_millis(300));
    down.failure = Some("the provider is down".to_owned());
    let second = rig_sharing(&first, down, limits(5_000, 30_000));

    let (outcome, background, _) = open(&second);

    assert_eq!(outcome, Ok(()));
    assert_eq!(second.player.played.lock().expect("lock").len(), 1);
    join_within(background, TIME_LIMIT);
    let title = saved(&second);
    assert!(
        !title.complete,
        "a failed search must not count as a complete list"
    );
    assert_eq!(title.links.len(), 3);
}

#[test]
fn closing_the_video_before_the_search_ends_saves_a_partial_list_of_what_worked() {
    let rig = rig(script(40, Duration::from_secs(1)), limits(5_000, 60_000));
    let (outcome, background, _) = open_skipping_after(&rig, Duration::from_millis(600));
    assert_eq!(outcome, Ok(()));

    rig.sources.close();
    join_within(background, TIME_LIMIT);

    let title = saved(&rig);
    assert!(!title.complete);
    assert!(!title.links.is_empty());
    assert!(title.links.len() < 40);
}

#[test]
fn a_video_that_fails_to_open_saves_nothing() {
    let silent = SearchScript::steady(1, Duration::from_secs(600), 720, Duration::from_secs(600));
    let rig = rig(silent, limits(300, 5_000));
    let (outcome, background, _) = open(&rig);
    assert!(outcome.is_err());
    join_within(background, TIME_LIMIT);
    assert!(matches!(
        rig.titles.cache.load(&title_key(&item())),
        Lookup::Miss(_)
    ));
}

#[test]
fn the_title_that_played_well_is_remembered_even_when_the_video_is_closed_at_once() {
    let first = open_first_time(3);
    let second = rig_sharing(
        &first,
        script(30, Duration::from_secs(SEARCH_SECONDS)),
        limits(5_000, 30_000),
    );
    let (outcome, background, _) = open(&second);
    assert_eq!(outcome, Ok(()));
    second.sources.close();
    join_within(background, TIME_LIMIT);
    let played = second.player.played.lock().expect("lock").clone();
    assert_eq!(
        saved(&second).last_good.as_deref(),
        played.first().map(String::as_str)
    );
}
