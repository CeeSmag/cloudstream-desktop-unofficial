//! A video that has played to its end is unloaded from mpv; Play or a seek after that must start
//! the same stream again instead of doing nothing (the viewer was stuck at the end of the movie).

use std::time::Duration;

use extension_runtime::fake_link_search::SearchScript;

use crate::opening_flow_tests::{limits, open, rig, Rig};
use crate::race_tests::{join_within, TIME_LIMIT};
use crate::recording_player::{ended, playing};
use crate::source_commands::restart_if_ended;

/// A video opened and fully checked, playing its best stream.
fn opened() -> Rig {
    let script = SearchScript::steady(3, Duration::from_millis(10), 720, Duration::from_millis(30));
    let rig = rig(script, limits(5_000, 30_000));
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    rig
}

fn report(rig: &Rig, status: player_api::PlaybackStatus) {
    *rig.player.reported.lock().expect("lock") = Some(status);
}

fn restart(rig: &Rig, position_ms: i64) -> Result<bool, String> {
    restart_if_ended(&rig.sources, &rig.services.playback, position_ms)
}

fn played(rig: &Rig) -> Vec<String> {
    rig.player.played.lock().expect("lock").clone()
}

fn positions(rig: &Rig) -> Vec<i64> {
    rig.player.positions.lock().expect("lock").clone()
}

#[test]
fn play_after_the_end_starts_the_same_stream_again_from_the_start() {
    let rig = opened();
    report(&rig, ended());

    assert_eq!(restart(&rig, 0), Ok(true));

    let urls = played(&rig);
    assert_eq!(urls.len(), 2);
    assert_eq!(urls[0], urls[1], "the same stream plays again");
    assert_eq!(positions(&rig)[1], 0);
}

#[test]
fn a_seek_after_the_end_plays_again_from_the_chosen_place() {
    let rig = opened();
    report(&rig, ended());

    assert_eq!(restart(&rig, 1_234_000), Ok(true));

    assert_eq!(positions(&rig)[1], 1_234_000);
}

#[test]
fn nothing_restarts_while_the_video_is_still_playing() {
    let rig = opened();
    report(&rig, playing());

    assert_eq!(restart(&rig, 0), Ok(false));

    assert_eq!(played(&rig).len(), 1);
}

#[test]
fn nothing_restarts_when_the_video_is_no_longer_open() {
    let rig = opened();
    report(&rig, ended());
    rig.sources.close();

    assert_eq!(restart(&rig, 0), Ok(false));

    assert_eq!(played(&rig).len(), 1);
}

#[test]
fn a_restart_of_the_selected_stream_keeps_it_selected() {
    let rig = opened();
    let before = rig.sources.session().snapshot().expect("open").selected;
    report(&rig, ended());

    assert_eq!(restart(&rig, 0), Ok(true));

    let after = rig.sources.session().snapshot().expect("open").selected;
    assert_eq!(before, after);
}
