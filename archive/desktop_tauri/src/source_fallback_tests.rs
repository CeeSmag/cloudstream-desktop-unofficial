//! The fallback with the whole opening flow behind it: a scripted provider, real checks, a player
//! that can fail, and saved sources on disk.

use std::sync::{Arc, Barrier};
use std::thread::JoinHandle;

use app_services::PlaybackItem;
use core_model::PlayableLink;
use extension_runtime::fake_link_search::SearchScript;
use title_cache::{LinkHealth, Lookup};

use super::*;
use crate::opening_flow_tests::{limits, open, rig, Rig};
use crate::race_tests::{join_within, TIME_LIMIT};
use crate::recording_player::{failed, playing};
use crate::video_opening::title_key;

use std::time::Duration;

/// A video of `count` working streams of the same quality, opened and fully checked.
fn opened(count: usize) -> Rig {
    let script = SearchScript::steady(
        count,
        Duration::from_millis(10),
        720,
        Duration::from_millis(30),
    );
    let rig = rig(script, limits(5_000, 30_000));
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    rig
}

fn fail(rig: &Rig, reason: &str) -> FallbackOutcome {
    rig.sources
        .fallback()
        .on_status(&rig.sources, &rig.services.playback, &failed(reason))
}

fn played(rig: &Rig) -> Vec<String> {
    rig.player.played.lock().expect("lock").clone()
}

fn the_title() -> String {
    title_key(&PlaybackItem {
        provider: "Fake".to_owned(),
        media_key: "title".to_owned(),
        ..PlaybackItem::default()
    })
}

#[test]
fn a_failed_stream_is_replaced_by_the_next_best_source_at_the_same_position() {
    let rig = opened(6);
    rig.sources.session().remember_position(42_000);

    assert_eq!(fail(&rig, "reason one"), FallbackOutcome::Switched(1));

    assert_eq!(played(&rig).len(), 2);
    assert_ne!(played(&rig)[0], played(&rig)[1]);
    assert_eq!(rig.player.positions.lock().expect("lock")[1], 42_000);
    let active = rig.sources.session().snapshot().expect("open");
    assert_eq!(active.selected, 1);
    assert_eq!(active.health[0], SourceHealth::Broken);
}

#[test]
fn the_failed_stream_is_marked_broken_in_the_saved_sources() {
    let rig = opened(4);
    fail(&rig, "reason one");
    let Lookup::Hit(hit) = rig.titles.cache.load(&the_title()) else {
        panic!("nothing saved");
    };
    let first = rig.sources.session().snapshot().expect("open").links[0]
        .url
        .clone();
    let broken: Vec<_> = hit
        .title
        .links
        .iter()
        .filter(|cached| cached.health == LinkHealth::Broken)
        .map(|cached| cached.link.url.clone())
        .collect();
    assert_eq!(broken, [first]);
}

#[test]
fn three_failures_give_up_with_one_message_that_names_every_reason() {
    let rig = opened(6);
    assert_eq!(fail(&rig, "first reason"), FallbackOutcome::Switched(1));
    assert_eq!(fail(&rig, "second reason"), FallbackOutcome::Switched(2));

    let FallbackOutcome::GaveUp(message) = fail(&rig, "third reason") else {
        panic!("the third failure must end the fallback");
    };

    for part in [
        "3 sources failed",
        "first reason",
        "second reason",
        "third reason",
    ] {
        assert!(message.contains(part), "{message}");
    }
    assert_eq!(played(&rig).len(), 3, "no fourth source is tried");
    assert_eq!(fail(&rig, "again"), FallbackOutcome::GaveUp(message));
    assert_eq!(played(&rig).len(), 3);
}

#[test]
fn when_there_is_no_other_source_it_gives_up_at_once_and_says_so() {
    let rig = opened(1);
    let FallbackOutcome::GaveUp(message) = fail(&rig, "it broke") else {
        panic!("nothing else to try");
    };
    assert!(
        message.contains("1 source failed") && message.contains("no other source"),
        "{message}"
    );
    assert_eq!(played(&rig).len(), 1);
}

#[test]
fn a_source_the_player_refuses_to_start_is_skipped_in_the_same_call() {
    let rig = opened(6);
    rig.player
        .refuses
        .lock()
        .expect("lock")
        .push("host1.".to_owned());

    assert_eq!(fail(&rig, "first reason"), FallbackOutcome::Switched(2));

    let active = rig.sources.session().snapshot().expect("open");
    assert_eq!(active.health[1], SourceHealth::Broken);
    assert_eq!(played(&rig).len(), 2, "the refused stream was never played");
}

#[test]
fn refusals_count_toward_the_attempt_limit() {
    let rig = opened(6);
    for word in ["host1.", "host2."] {
        rig.player
            .refuses
            .lock()
            .expect("lock")
            .push(word.to_owned());
    }
    let FallbackOutcome::GaveUp(message) = fail(&rig, "first reason") else {
        panic!("three sources failed");
    };
    assert!(message.contains("3 sources failed"), "{message}");
    assert!(message.contains("refused in this test"), "{message}");
}

#[test]
fn a_stream_that_plays_ends_the_run_and_is_remembered_as_the_one_that_worked() {
    let rig = opened(6);
    for round in 1..=4 {
        assert_eq!(
            fail(&rig, "broke"),
            FallbackOutcome::Switched(round),
            "round {round}"
        );
        let status = playing();
        rig.sources
            .fallback()
            .on_status(&rig.sources, &rig.services.playback, &status);
    }
    let Lookup::Hit(hit) = rig.titles.cache.load(&the_title()) else {
        panic!("nothing saved");
    };
    assert_eq!(
        hit.title.last_good.as_deref(),
        played(&rig).last().map(String::as_str)
    );
}

#[test]
fn a_manual_switch_starts_the_count_over() {
    let rig = opened(6);
    fail(&rig, "one");
    fail(&rig, "two");
    crate::source_commands::switch_to(&rig.sources, &rig.services.playback, 4).expect("switched");
    assert!(matches!(fail(&rig, "three"), FallbackOutcome::Switched(_)));
}

#[test]
fn trying_for_longer_than_the_time_limit_gives_up() {
    let rig = opened(6);
    let controller = FallbackController::new(FallbackLimits {
        attempts: 10,
        total: Duration::from_millis(50),
    });
    let status = failed("slow");
    let outcome = controller.on_status(&rig.sources, &rig.services.playback, &status);
    assert!(matches!(outcome, FallbackOutcome::Switched(_)));
    std::thread::sleep(Duration::from_millis(80));
    let FallbackOutcome::GaveUp(message) =
        controller.on_status(&rig.sources, &rig.services.playback, &status)
    else {
        panic!("the time is up");
    };
    assert!(message.contains("took too long"), "{message}");
}

fn link(url: &str, quality: i32) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        quality,
        ..PlayableLink::default()
    }
}

#[test]
fn working_sources_are_tried_before_ones_still_being_checked_and_broken_ones_never() {
    let links = vec![
        link("a", 720),
        link("b", 2160),
        link("c", 480),
        link("d", 1080),
    ];
    let mut active = ActiveSources::new(PlaybackItem::default(), links, Vec::new());
    active.health = vec![
        SourceHealth::Working,
        SourceHealth::Checking,
        SourceHealth::Working,
        SourceHealth::Broken,
    ];
    assert_eq!(
        next_candidate(&active, 0),
        Some(2),
        "working before checking"
    );
    active.health[2] = SourceHealth::Broken;
    assert_eq!(
        next_candidate(&active, 0),
        Some(1),
        "checking before nothing"
    );
    active.health[1] = SourceHealth::Broken;
    assert_eq!(next_candidate(&active, 0), None);
}

#[test]
fn states_that_are_not_failures_change_nothing() {
    let rig = opened(3);
    for state in [
        PlaybackState::Idle,
        PlaybackState::Loading,
        PlaybackState::Ended,
    ] {
        let status = PlaybackStatus {
            state,
            ..PlaybackStatus::default()
        };
        let outcome =
            rig.sources
                .fallback()
                .on_status(&rig.sources, &rig.services.playback, &status);
        assert_eq!(outcome, FallbackOutcome::Fine);
    }
    assert_eq!(played(&rig).len(), 1);
}

#[test]
fn closing_the_video_ends_the_fallback() {
    let rig = opened(3);
    rig.sources.close();
    assert_eq!(fail(&rig, "late"), FallbackOutcome::Fine);
    assert_eq!(played(&rig).len(), 1);
}

#[test]
fn a_new_video_does_not_inherit_the_failures_of_the_one_before() {
    let rig = opened(6);
    fail(&rig, "one");
    fail(&rig, "two");
    assert!(matches!(fail(&rig, "three"), FallbackOutcome::GaveUp(_)));
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    join_within(background, TIME_LIMIT);
    assert!(
        matches!(fail(&rig, "again"), FallbackOutcome::Switched(_)),
        "the second video must start its own count"
    );
}

#[path = "source_fallback_race_tests.rs"]
mod races;
