//! Leaving the player and coming back, against a real manager and session (T1), and race tests:
//! a video that starts opening while the viewer leaves or comes back is never paused or resumed.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

use app_services::StreamChecker;
use extension_repo::{HttpProbe, ProbeResponse, RepoError};

use super::*;
use crate::race_tests::{join_within, TIME_LIMIT};

struct Unreachable;

impl HttpProbe for Unreachable {
    fn probe(&self, url: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        Err(RepoError::Download {
            url: url.to_owned(),
            reason: "unreachable".to_owned(),
        })
    }
}

fn manager() -> SourceManager {
    SourceManager::new(
        StreamChecker::new(Arc::new(Unreachable)),
        crate::test_subtitles::unused_loader(),
        crate::test_titles::unused_titles(),
    )
}

fn video(key: &str) -> PlaybackItem {
    PlaybackItem {
        provider: "P".to_owned(),
        media_key: key.to_owned(),
        ..PlaybackItem::default()
    }
}

/// Opens `item` the way `play_media` does (its wait, then its list).
fn open(manager: &SourceManager, item: PlaybackItem) -> u64 {
    let gate = manager.begin();
    manager.open(&gate, item).expect("opened")
}

fn ran(done: &AtomicBool) -> impl FnOnce() -> Result<(), String> + '_ {
    move || {
        done.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn leaving_marks_the_open_video_waiting_and_pauses_it() {
    let manager = manager();
    open(&manager, video("a"));
    let paused = AtomicBool::new(false);
    assert_eq!(manager.leave_with(ran(&paused)), Ok(true));
    assert!(paused.load(Ordering::SeqCst));
    assert!(manager.session().snapshot().expect("open").away);
    assert!(manager.with_away(|away| away));
}

#[test]
fn nothing_is_paused_when_nothing_is_open_or_another_video_started_opening() {
    let manager = manager();
    let paused = AtomicBool::new(false);
    assert_eq!(manager.leave_with(ran(&paused)), Ok(false), "nothing open");
    open(&manager, video("a"));
    let _other = manager.begin(); // video B starts opening; A's list is still there
    assert_eq!(manager.leave_with(ran(&paused)), Ok(false));
    assert!(!paused.load(Ordering::SeqCst), "B would have been paused");
}

#[test]
fn coming_back_to_the_waiting_video_resumes_it_and_marks_it_watched() {
    let manager = manager();
    open(&manager, video("a"));
    manager.leave_with(|| Ok(())).expect("left");
    let resumed = AtomicBool::new(false);
    assert_eq!(manager.come_back_with(&video("a"), ran(&resumed)), Ok(true));
    assert!(resumed.load(Ordering::SeqCst));
    assert!(!manager.with_away(|away| away));
}

#[test]
fn another_video_or_one_that_was_not_left_is_not_resumed() {
    let manager = manager();
    open(&manager, video("a"));
    let resumed = AtomicBool::new(false);
    assert_eq!(
        manager.come_back_with(&video("a"), ran(&resumed)),
        Ok(false),
        "the viewer never left"
    );
    manager.leave_with(|| Ok(())).expect("left");
    assert_eq!(
        manager.come_back_with(&video("b"), ran(&resumed)),
        Ok(false)
    );
    assert!(!resumed.load(Ordering::SeqCst));
    assert!(manager.with_away(|away| away), "A still waits");
}

#[test]
fn a_failed_pause_is_reported() {
    let manager = manager();
    open(&manager, video("a"));
    let failed = manager.leave_with(|| Err("mpv refused".to_owned()));
    assert_eq!(failed, Err("mpv refused".to_owned()));
}

#[test]
fn playing_another_video_forgets_that_one_waited() {
    let manager = manager();
    open(&manager, video("a"));
    manager.leave_with(|| Ok(())).expect("left");
    open(&manager, video("b"));
    assert!(!manager.with_away(|away| away));
    let resumed = AtomicBool::new(false);
    assert_eq!(
        manager.come_back_with(&video("a"), ran(&resumed)),
        Ok(false)
    );
}

/// Counts in `wrong` a pause or resume that finds the open list not marked `away`. It first gives
/// the other thread every chance to open a video right now: with the lock held it cannot.
fn check_list(manager: &SourceManager, wrong: &AtomicUsize, away: bool) -> Result<(), String> {
    for _ in 0..20 {
        std::thread::yield_now();
    }
    let active = manager.session().snapshot();
    if active.is_none_or(|active| active.away != away) {
        wrong.fetch_add(1, Ordering::SeqCst);
    }
    Ok(())
}

const ROUNDS: usize = 1_000;

fn spawn_viewer(
    manager: Arc<SourceManager>,
    wrong: Arc<AtomicUsize>,
    start: Arc<Barrier>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        start.wait();
        for _ in 0..ROUNDS {
            let _ = manager.leave_with(|| check_list(&manager, &wrong, true));
            let _ = manager.come_back_with(&video("a"), || check_list(&manager, &wrong, false));
        }
    })
}

/// One thread leaves and comes back again and again while another opens new videos. Whenever the
/// pause or the resume runs, the open list must be the one it was meant for: marked waiting for a
/// pause, marked watched for a resume. All threads must end.
#[test]
fn race_leaving_and_coming_back_while_new_videos_open() {
    let manager = Arc::new(manager());
    open(&manager, video("a"));
    let wrong = Arc::new(AtomicUsize::new(0));
    let start = Arc::new(Barrier::new(2));
    let viewer = spawn_viewer(Arc::clone(&manager), Arc::clone(&wrong), Arc::clone(&start));
    let opener = {
        let manager = Arc::clone(&manager);
        std::thread::spawn(move || {
            start.wait();
            for round in 0..ROUNDS {
                open(&manager, video(if round % 2 == 0 { "b" } else { "a" }));
            }
        })
    };
    join_within(vec![viewer, opener], TIME_LIMIT);
    assert_eq!(
        wrong.load(Ordering::SeqCst),
        0,
        "a pause or resume hit the wrong list"
    );
}
