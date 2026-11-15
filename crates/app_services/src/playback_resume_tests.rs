use player_api::{PlaybackState, PlaybackStatus};

use super::*;
use crate::playback_types::NextEpisode;

fn episode_one() -> PlaybackItem {
    PlaybackItem {
        provider: "P".into(),
        media_key: "ep1".into(),
        title: "Show".into(),
        detail_url: "https://show/1".into(),
        poster_url: Some("p.png".into()),
        season: Some(1),
        episode: Some(1),
        next_episode: Some(NextEpisode {
            media_key: "ep2".into(),
            season: Some(1),
            episode: Some(2),
        }),
    }
}

fn status(state: PlaybackState, position_ms: i64) -> PlaybackStatus {
    PlaybackStatus {
        state,
        position_ms,
        duration_ms: 100_000,
        ..PlaybackStatus::default()
    }
}

fn saved_episode(change: ResumeChange) -> Option<(String, Option<i32>, i64)> {
    match change {
        ResumeChange::Save(pointer) => {
            Some((pointer.media_key, pointer.episode, pointer.updated_at_ms))
        }
        _ => None,
    }
}

#[test]
fn is_over_watch_threshold_89_90_91_percent_and_unknown_length() {
    assert!(!is_over_watch_threshold(89_000, 100_000));
    assert!(is_over_watch_threshold(90_000, 100_000));
    assert!(is_over_watch_threshold(91_000, 100_000));
    assert!(!is_over_watch_threshold(5_000, 0));
}

#[test]
fn resume_change_under_90_percent_points_at_this_episode() {
    let change = resume_change(&episode_one(), &status(PlaybackState::Playing, 50_000), 7);
    assert_eq!(saved_episode(change), Some(("ep1".into(), Some(1), 7)));
}

#[test]
fn resume_change_at_90_percent_points_at_the_next_episode() {
    let change = resume_change(&episode_one(), &status(PlaybackState::Playing, 90_000), 7);
    assert_eq!(saved_episode(change), Some(("ep2".into(), Some(2), 7)));
}

#[test]
fn resume_change_ended_early_still_points_at_the_next_episode() {
    let change = resume_change(&episode_one(), &status(PlaybackState::Ended, 10_000), 7);
    assert_eq!(saved_episode(change), Some(("ep2".into(), Some(2), 7)));
}

#[test]
fn resume_change_last_episode_done_removes_the_pointer() {
    let last = PlaybackItem {
        next_episode: None,
        ..episode_one()
    };
    let change = resume_change(&last, &status(PlaybackState::Playing, 95_000), 7);
    assert_eq!(change, ResumeChange::Remove);
}

#[test]
fn resume_change_without_title_page_leaves_the_pointer_alone() {
    let loose = PlaybackItem {
        detail_url: String::new(),
        ..episode_one()
    };
    let change = resume_change(&loose, &status(PlaybackState::Ended, 100_000), 7);
    assert_eq!(change, ResumeChange::Nothing);
}
