//! The whole opening of a video with subtitles: the provider names files, the app downloads them
//! in the background, and the video and the player only ever get clean local files.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use extension_runtime::fake_link_search::SearchScript;

use crate::opening_flow_tests::{grace, open, rig, Rig};
use crate::race_tests::{join_within, TIME_LIMIT};

fn script_with_subtitles(languages: &[(&str, u64)]) -> SearchScript {
    let mut script = SearchScript::steady(
        3,
        Duration::from_millis(60),
        720,
        Duration::from_millis(900),
    );
    script.subtitles = languages
        .iter()
        .map(|(language, at_ms)| (Duration::from_millis(*at_ms), (*language).to_owned()))
        .collect();
    script
}

fn fast_grace() -> crate::check_gate::Grace {
    grace(20, 40, 1_000)
}

fn subtitles_of(rig: &Rig) -> Vec<(String, String)> {
    rig.sources
        .session()
        .snapshot()
        .map(|active| {
            active
                .subtitles
                .into_iter()
                .map(|track| (track.lang, track.url))
                .collect()
        })
        .unwrap_or_default()
}

fn wait_for(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn subtitle_files_named_during_the_search_become_local_files_of_the_open_video() {
    let rig = rig(
        script_with_subtitles(&[("Arabic", 30), ("English", 90)]),
        fast_grace(),
    );

    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    wait_for("both subtitle files", || subtitles_of(&rig).len() == 2);

    let found = subtitles_of(&rig);
    let languages: Vec<&str> = found
        .iter()
        .map(|(language, _)| language.as_str())
        .collect();
    assert!(
        languages.contains(&"Arabic") && languages.contains(&"English"),
        "{languages:?}"
    );
    for (_, url) in &found {
        assert!(
            !url.starts_with("http") && Path::new(url).exists(),
            "not a local file: {url}"
        );
    }
    assert_eq!(rig.subtitles.host.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn subtitles_found_after_playback_started_are_given_to_the_player_that_is_playing() {
    // The first stream is there at once; the subtitle is found a second later.
    let rig = rig(script_with_subtitles(&[("Arabic", 700)]), fast_grace());

    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    assert!(
        rig.player.started_with.lock().expect("lock")[0].is_empty(),
        "the subtitle did not exist yet when the stream started"
    );
    wait_for("the subtitle to reach the player", || {
        !rig.player.added_later.lock().expect("lock").is_empty()
    });

    let given = rig.player.added_later.lock().expect("lock").clone();
    assert_eq!(given.len(), 1);
    assert_eq!(given[0].language, "Arabic");
    assert!(
        Path::new(&given[0].url).exists(),
        "the player must get a local file"
    );
    rig.sources.close();
    join_within(background, TIME_LIMIT);
}

#[test]
fn a_dead_subtitle_is_skipped_and_the_others_still_arrive() {
    let rig = rig(
        script_with_subtitles(&[("dead", 20), ("Arabic", 40), ("English", 60)]),
        fast_grace(),
    );

    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    wait_for("the two live subtitles", || subtitles_of(&rig).len() == 2);

    let languages: Vec<String> = subtitles_of(&rig)
        .into_iter()
        .map(|(language, _)| language)
        .collect();
    assert!(!languages.contains(&"dead".to_owned()), "{languages:?}");
}

/// The case behind the bug report: the provider's subtitle service answered on the first open and
/// did not on the second. The title must keep what it was given.
#[test]
fn a_title_opened_again_gets_its_saved_subtitles_at_once_even_when_the_provider_offers_none() {
    let rig = rig(script_with_subtitles(&[("Arabic", 30)]), fast_grace());
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    wait_for("the Arabic file", || subtitles_of(&rig).len() == 1);
    rig.sources.close();

    // Second time: the provider finds streams but no subtitle at all.
    let again =
        crate::opening_flow_tests::rig_sharing(&rig, script_with_subtitles(&[]), fast_grace());
    let (outcome, background, _) = open(&again);
    assert_eq!(outcome, Ok(()));

    let started_with = again.player.started_with.lock().expect("lock")[0].clone();
    assert_eq!(
        started_with.len(),
        1,
        "the saved Arabic subtitle was not offered at the start"
    );
    assert_eq!(started_with[0].language, "Arabic");
    assert!(Path::new(&started_with[0].url).exists());
    again.sources.close();
    join_within(background, TIME_LIMIT);
    assert_eq!(
        rig.subtitles.host.calls.load(Ordering::SeqCst),
        1,
        "nothing is downloaded the second time"
    );
}

#[test]
fn a_subtitle_the_provider_offers_again_is_not_listed_twice_or_downloaded_again() {
    let rig = rig(script_with_subtitles(&[("Arabic", 30)]), fast_grace());
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    wait_for("the Arabic file", || subtitles_of(&rig).len() == 1);
    rig.sources.close();

    let again = crate::opening_flow_tests::rig_sharing(
        &rig,
        script_with_subtitles(&[("Arabic", 30)]),
        fast_grace(),
    );
    let (outcome, background, _) = open(&again);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);

    assert_eq!(
        subtitles_of(&again).len(),
        1,
        "the same subtitle was listed twice"
    );
    assert_eq!(
        rig.subtitles.host.calls.load(Ordering::SeqCst),
        1,
        "the file was downloaded again"
    );
}

#[test]
fn leaving_stops_the_subtitle_downloads_that_have_not_started() {
    let slow: Vec<(&str, u64)> = (0..40).map(|n| ("slow", 20 + n)).collect();
    let rig = rig(script_with_subtitles(&slow), fast_grace());
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    wait_for("the first downloads to start", || {
        rig.subtitles.host.calls.load(Ordering::SeqCst) > 0
    });

    rig.sources.close();

    join_within(background, TIME_LIMIT);
    let started = rig.subtitles.host.calls.load(Ordering::SeqCst);
    assert!(
        started <= 16,
        "{started} of 40 downloads were started although the viewer had left"
    );
}
