//! Only the video opened last keeps saved sources and subtitles: opening another one drops those of
//! the previous video, so it searches and downloads again when it is opened again (T2, with a race
//! test for the previous video's checks ending after the next one was opened).

use std::sync::atomic::Ordering;

use title_cache::Lookup;

use super::*;
use crate::video_opening::title_key;

fn video(name: &str) -> PlaybackItem {
    PlaybackItem {
        provider: "Fake".to_owned(),
        media_key: name.to_owned(),
        ..PlaybackItem::default()
    }
}

fn open_one(rig: &Rig, item: PlaybackItem) -> (Result<(), String>, Vec<JoinHandle<()>>, Duration) {
    let tell: Tell = Arc::new(|_| {});
    let started = Instant::now();
    let (outcome, background) = open_video(&rig.sources, &rig.services, item, tell);
    (outcome, background, started.elapsed())
}

/// Opens `item` with the viewer pressing skip `pause` after; the search goes on in the background.
fn open_one_skipping(
    rig: &Rig,
    item: PlaybackItem,
    pause: Duration,
) -> (Result<(), String>, Vec<JoinHandle<()>>) {
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(pause);
            rig.sources.skip_with(&|_| {});
        });
        let (outcome, background, _) = open_one(rig, item);
        (outcome, background)
    })
}

/// Opens `item`, waits for everything to finish and closes it again.
fn watch_to_the_end(rig: &Rig, item: PlaybackItem) -> Duration {
    let (outcome, background, waited) = open_one(rig, item);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    rig.sources.close();
    waited
}

fn has_saved_sources(rig: &Rig, item: &PlaybackItem) -> bool {
    matches!(rig.titles.cache.load(&title_key(item)), Lookup::Hit(_))
}

fn saved_subtitles(rig: &Rig, item: &PlaybackItem) -> usize {
    rig.subtitles
        .loader
        .cache()
        .cached_for_title(&title_key(item))
        .len()
}

/// Three streams in 180 ms and one subtitle file; the search ends 300 ms after the last stream.
fn search_with_a_subtitle() -> SearchScript {
    let mut script = SearchScript::steady(
        3,
        Duration::from_millis(60),
        720,
        Duration::from_millis(300),
    );
    script.subtitles = vec![(Duration::from_millis(30), "English".to_owned())];
    script
}

#[test]
fn opening_another_video_drops_the_saved_sources_and_subtitles_of_the_previous_one() {
    let rig = rig(search_with_a_subtitle(), limits(5_000, 20_000));
    let (first, second) = (video("episode-1"), video("episode-2"));
    watch_to_the_end(&rig, first.clone());
    assert!(has_saved_sources(&rig, &first));
    assert_eq!(saved_subtitles(&rig, &first), 1);

    watch_to_the_end(&rig, second.clone());

    assert!(
        !has_saved_sources(&rig, &first),
        "the previous video's sources must go"
    );
    assert_eq!(saved_subtitles(&rig, &first), 0, "and its subtitles");
    assert!(has_saved_sources(&rig, &second));
    assert_eq!(saved_subtitles(&rig, &second), 1);
}

#[test]
fn a_video_opened_again_after_another_one_searches_and_downloads_again() {
    let rig = rig(search_with_a_subtitle(), limits(5_000, 20_000));
    let (first, second) = (video("episode-1"), video("episode-2"));
    watch_to_the_end(&rig, first.clone());
    watch_to_the_end(&rig, second.clone());
    let downloads_before = rig.subtitles.host.calls.load(Ordering::SeqCst);

    let waited = watch_to_the_end(&rig, first.clone());

    assert!(
        waited >= Duration::from_millis(400),
        "played after {waited:?}: saved sources were used instead of a new search"
    );
    assert_eq!(
        rig.subtitles.host.calls.load(Ordering::SeqCst),
        downloads_before + 1,
        "the subtitle is downloaded again"
    );
    assert!(
        !has_saved_sources(&rig, &second),
        "now the other one is dropped"
    );
}

#[test]
fn the_video_opened_last_still_plays_at_once_from_its_saved_sources() {
    let rig = rig(search_with_a_subtitle(), limits(5_000, 20_000));
    let first = video("episode-1");
    watch_to_the_end(&rig, first.clone());

    let waited = watch_to_the_end(&rig, first.clone());

    assert!(waited < Duration::from_millis(300), "waited {waited:?}");
    assert!(has_saved_sources(&rig, &first));
}

/// The viewer skips into video A and opens video B while A's search and checks still run, again
/// and again. A's checks end (and try to save) around the time B is opened; whatever the order,
/// A has no saved sources afterwards.
#[test]
fn race_the_previous_videos_checks_ending_late_never_bring_its_sources_back() {
    const ROUNDS: usize = 6;
    let script = SearchScript::steady(
        10,
        Duration::from_millis(20),
        720,
        Duration::from_millis(300),
    );
    let rig = rig(script, limits(5_000, 20_000));
    let (first, second) = (video("episode-1"), video("episode-2"));
    for round in 0..ROUNDS {
        let (outcome, first_work) =
            open_one_skipping(&rig, first.clone(), Duration::from_millis(40));
        assert_eq!(outcome, Ok(()), "round {round}");
        let (outcome, second_work) =
            open_one_skipping(&rig, second.clone(), Duration::from_millis(40));
        assert_eq!(outcome, Ok(()), "round {round}");
        join_within(first_work, TIME_LIMIT);
        assert!(
            !has_saved_sources(&rig, &first),
            "round {round}: video A's sources came back after B was opened"
        );
        rig.sources.close();
        join_within(second_work, TIME_LIMIT);
        assert_eq!(
            rig.searches.stats().active,
            0,
            "round {round}: a search was left running"
        );
    }
}
