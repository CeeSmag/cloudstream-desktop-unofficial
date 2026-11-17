//! Refreshing the sources of the playing video, against the scripted provider (T2): the saved
//! sources go, the provider searches again, the stream that plays goes on, nothing is listed or
//! handed to the player twice, and a refresh never disturbs another video that is opening.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use app_services::PlaybackItem;
use extension_runtime::fake_link_search::SearchScript;
use title_cache::Lookup;

use super::*;
use crate::opening_flow_tests::{limits, open, open_skipping_after, rig, Rig};
use crate::race_tests::{join_within, TIME_LIMIT};
use crate::video_opening::{open_video, title_key};

fn the_title() -> String {
    title_key(&PlaybackItem {
        provider: "Fake".to_owned(),
        media_key: "title".to_owned(),
        ..PlaybackItem::default()
    })
}

fn quiet() -> Tell {
    Arc::new(|_| {})
}

fn is_saved(rig: &Rig) -> bool {
    matches!(rig.titles.cache.load(&the_title()), Lookup::Hit(_))
}

fn listed(rig: &Rig) -> Vec<String> {
    rig.sources
        .session()
        .snapshot()
        .map(|active| active.links.into_iter().map(|link| link.url).collect())
        .unwrap_or_default()
}

/// Three streams in 180 ms and one subtitle file; the search ends 300 ms after the last stream.
fn short_search() -> SearchScript {
    let mut script = SearchScript::steady(
        3,
        Duration::from_millis(60),
        720,
        Duration::from_millis(300),
    );
    script.subtitles = vec![(Duration::from_millis(30), "English".to_owned())];
    script
}

/// Opens the title, waits for its search and checks to end, and leaves it playing.
fn playing_after_everything(script: SearchScript) -> Rig {
    let rig = rig(script, limits(5_000, 20_000));
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    rig
}

fn subtitles_given(rig: &Rig) -> usize {
    let at_start: usize = rig
        .player
        .started_with
        .lock()
        .expect("lock")
        .iter()
        .map(Vec::len)
        .sum();
    at_start + rig.player.added_later.lock().expect("lock").len()
}

fn wait_for(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_refresh_drops_the_saved_sources_and_searches_again_while_the_stream_plays() {
    let rig = playing_after_everything(short_search());
    assert!(is_saved(&rig));
    let playing = rig.player.played.lock().expect("lock")[0].clone();
    let searches_before = rig.searches.stats().started;

    let background = refresh_playing(&rig.sources, &rig.services, quiet()).expect("refreshed");

    assert!(!is_saved(&rig), "the saved sources are dropped at once");
    assert_eq!(
        listed(&rig),
        std::slice::from_ref(&playing),
        "only the stream that plays stays"
    );
    assert_eq!(rig.searches.stats().started, searches_before + 1);
    join_within(background, TIME_LIMIT);
    assert_eq!(
        listed(&rig).len(),
        3,
        "the stream that plays is not listed twice"
    );
    let active = rig.sources.session().snapshot().expect("open");
    assert!(active.playing);
    assert_eq!(active.links[active.selected].url, playing);
    assert_eq!(
        rig.player.played.lock().expect("lock").len(),
        1,
        "playback was not restarted"
    );
    assert!(is_saved(&rig), "the fresh search is saved when it ends");
    rig.sources.close();
}

#[test]
fn the_subtitles_the_player_has_are_not_given_again_after_a_refresh() {
    let rig = playing_after_everything(short_search());
    wait_for("the subtitle to reach the player", || {
        subtitles_given(&rig) == 1
    });

    let background = refresh_playing(&rig.sources, &rig.services, quiet()).expect("refreshed");
    join_within(background, TIME_LIMIT);

    assert_eq!(
        subtitles_given(&rig),
        1,
        "the same file was handed over again"
    );
    let active = rig.sources.session().snapshot().expect("open");
    assert_eq!(active.subtitles.len(), 1);
    rig.sources.close();
}

#[test]
fn nothing_is_refreshed_while_nothing_plays() {
    let rig = rig(short_search(), limits(5_000, 20_000));
    let refused = refresh_playing(&rig.sources, &rig.services, quiet()).err();
    assert_eq!(refused.as_deref(), Some(NOTHING_PLAYS));
    assert_eq!(rig.searches.stats().started, 0, "no search for nothing");
}

/// Another video started opening (its wait began) between the refresh's look at the list and the
/// restart of the list: the refresh gives up and stops the search it started.
#[test]
fn a_refresh_is_refused_when_another_video_started_opening_meanwhile() {
    let rig = playing_after_everything(short_search());
    let other_opening = rig.sources.begin();

    let refused = refresh_playing(&rig.sources, &rig.services, quiet()).err();

    assert_eq!(refused.as_deref(), Some(VIDEO_CHANGED));
    assert!(
        !other_opening.is_cancelled(),
        "the other video's opening must go on"
    );
    assert_eq!(
        rig.searches.stats().active,
        0,
        "the refresh's search was stopped"
    );
    rig.sources.close();
}

/// The viewer refreshes while the first search still runs: its checks end afterwards and try to
/// save, which must not bring back what the refresh dropped.
#[test]
fn the_old_search_cannot_save_after_a_refresh() {
    let script = SearchScript::steady(
        10,
        Duration::from_millis(20),
        720,
        Duration::from_millis(600),
    );
    let rig = rig(script, limits(5_000, 20_000));
    let (outcome, old_work, _) = open_skipping_after(&rig, Duration::from_millis(60));
    assert_eq!(outcome, Ok(()));

    let new_work = refresh_playing(&rig.sources, &rig.services, quiet()).expect("refreshed");
    join_within(old_work, TIME_LIMIT);

    assert_eq!(
        rig.searches.stats().active,
        1,
        "the fresh search still runs"
    );
    assert!(!is_saved(&rig), "the old search saved after the refresh");
    join_within(new_work, TIME_LIMIT);
    assert!(is_saved(&rig));
    rig.sources.close();
}

fn open_other_skipping(rig: &Rig) -> (Result<(), String>, Vec<JoinHandle<()>>) {
    let item = PlaybackItem {
        provider: "Fake".to_owned(),
        media_key: "other".to_owned(),
        ..PlaybackItem::default()
    };
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(40));
            rig.sources.skip_with(&|_| {});
        });
        open_video(&rig.sources, &rig.services, item, quiet())
    })
}

/// A refresh and the opening of another video at the same time, many times: the other video
/// always opens and is the one left open, and nothing keeps running after it closes.
#[test]
fn race_a_refresh_never_breaks_the_opening_of_another_video() {
    const ROUNDS: usize = 5;
    let script = SearchScript::steady(
        6,
        Duration::from_millis(20),
        720,
        Duration::from_millis(200),
    );
    let rig = rig(script, limits(5_000, 20_000));
    for round in 0..ROUNDS {
        let (outcome, first_work, _) = open_skipping_after(&rig, Duration::from_millis(40));
        assert_eq!(outcome, Ok(()), "round {round}");
        let (refreshed, (opened, other_work)) = std::thread::scope(|scope| {
            let refreshing = scope.spawn(|| refresh_playing(&rig.sources, &rig.services, quiet()));
            let opened = open_other_skipping(&rig);
            (refreshing.join().expect("refresh panicked"), opened)
        });
        assert_eq!(
            opened,
            Ok(()),
            "round {round}: the other video did not open"
        );
        let open_item = rig
            .sources
            .session()
            .snapshot()
            .map(|active| active.item.media_key);
        assert_eq!(open_item.as_deref(), Some("other"), "round {round}");
        rig.sources.close();
        join_within(first_work, TIME_LIMIT);
        join_within(other_work, TIME_LIMIT);
        join_within(refreshed.unwrap_or_default(), TIME_LIMIT);
        assert_eq!(
            rig.searches.stats().active,
            0,
            "round {round}: a search was left running"
        );
    }
}
