//! Leaving the player and coming back through the whole opening against the scripted provider
//! (T2): the video waits paused with half the cache, opening it again plays on without a new search
//! and with the full cache, and anything else is opened or stopped as before.

use std::time::Duration;

use app_services::PlaybackItem;
use extension_runtime::fake_link_search::SearchScript;

use super::*;
use crate::opening_flow_tests::{limits, open_skipping_after, rig, Rig};
use crate::race_tests::{join_within, TIME_LIMIT};
use crate::recording_player::{ended, playing};

const MB: u64 = 1024 * 1024;

fn the_video() -> PlaybackItem {
    PlaybackItem {
        provider: "Fake".to_owned(),
        media_key: "title".to_owned(),
        ..PlaybackItem::default()
    }
}

/// 2 GB, 75 % ahead: 1536/512 MB while watching, 768/256 MB while away.
fn two_gb() -> VideoCache {
    VideoCache {
        size_mb: 2048,
        ahead_percent: 75,
    }
}

/// Opens the video (skipping the wait) and has the player report it plays.
fn playing_video() -> (Rig, Vec<std::thread::JoinHandle<()>>) {
    let script = SearchScript::steady(
        3,
        Duration::from_millis(20),
        720,
        Duration::from_millis(100),
    );
    let rig = rig(script, limits(5_000, 20_000));
    let (outcome, background, _) = open_skipping_after(&rig, Duration::from_millis(80));
    assert_eq!(outcome, Ok(()));
    *rig.player.reported.lock().expect("lock") = Some(playing());
    (rig, background)
}

fn calls(rig: &Rig) -> Vec<String> {
    std::mem::take(&mut *rig.player.calls.lock().expect("lock"))
}

#[test]
fn a_video_left_waits_paused_with_half_the_cache_and_opening_it_again_plays_on() {
    let (rig, background) = playing_video();
    let playback = &rig.services.playback;
    calls(&rig);

    leave_player(&rig.sources, playback, two_gb()).expect("left");
    assert_eq!(
        calls(&rig),
        ["paused true", &format!("cache {} {}", 768 * MB, 256 * MB)]
    );
    assert!(
        rig.sources.session().snapshot().is_some(),
        "its sources are kept"
    );

    let searches = rig.searches.stats().started;
    assert_eq!(
        come_back_to(&rig.sources, playback, &the_video(), two_gb()),
        Ok(true)
    );
    assert_eq!(
        calls(&rig),
        [&format!("cache {} {}", 1536 * MB, 512 * MB), "paused false"]
    );
    assert_eq!(rig.searches.stats().started, searches, "no new search");
    assert_eq!(
        rig.player.played.lock().expect("lock").len(),
        1,
        "not opened again"
    );
    rig.sources.close();
    join_within(background, TIME_LIMIT);
}

#[test]
fn another_video_is_not_taken_for_the_one_that_waits() {
    let (rig, background) = playing_video();
    leave_player(&rig.sources, &rig.services.playback, two_gb()).expect("left");
    let other = PlaybackItem {
        media_key: "other".to_owned(),
        ..the_video()
    };
    assert_eq!(
        come_back_to(&rig.sources, &rig.services.playback, &other, two_gb()),
        Ok(false)
    );
    rig.sources.close();
    join_within(background, TIME_LIMIT);
}

#[test]
fn a_video_that_ended_is_stopped_on_leaving_and_opened_again_afterwards() {
    let (rig, background) = playing_video();
    *rig.player.reported.lock().expect("lock") = Some(ended());
    calls(&rig);

    leave_player(&rig.sources, &rig.services.playback, two_gb()).expect("left");

    assert!(
        rig.sources.session().snapshot().is_none(),
        "stopped as before"
    );
    assert!(calls(&rig).is_empty(), "nothing to pause or shrink");
    assert_eq!(
        come_back_to(&rig.sources, &rig.services.playback, &the_video(), two_gb()),
        Ok(false)
    );
    join_within(background, TIME_LIMIT);
}

#[test]
fn a_video_whose_stream_is_gone_is_opened_again_rather_than_resumed() {
    let (rig, background) = playing_video();
    leave_player(&rig.sources, &rig.services.playback, two_gb()).expect("left");
    *rig.player.reported.lock().expect("lock") = None; // idle: mpv lost the stream meanwhile
    assert_eq!(
        come_back_to(&rig.sources, &rig.services.playback, &the_video(), two_gb()),
        Ok(false)
    );
    rig.sources.close();
    join_within(background, TIME_LIMIT);
}
