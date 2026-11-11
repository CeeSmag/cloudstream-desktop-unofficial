use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::*;

/// A reading that is up to date with the viewer's pause/play commands.
fn fresh(tracker: &StatusTracker, position_ms: i64, duration_ms: i64, paused: bool) -> Reading {
    Reading {
        position_ms,
        duration_ms,
        paused,
        pause_commands: tracker.pause_commands(),
        ..Reading::default()
    }
}

fn playing_tracker() -> StatusTracker {
    let tracker = StatusTracker::default();
    tracker.apply(MpvEvent::FileLoaded);
    tracker
}

#[test]
fn file_loaded_moves_loading_to_playing_and_progress_updates_apply() {
    let tracker = StatusTracker::default();
    tracker.begin_loading();
    assert_eq!(tracker.snapshot().state, PlaybackState::Loading);
    tracker.apply(MpvEvent::FileLoaded);
    tracker.update_progress(fresh(&tracker, 1_500, 9_000, false));
    let snapshot = tracker.snapshot();
    assert_eq!(
        (snapshot.state, snapshot.position_ms, snapshot.duration_ms),
        (PlaybackState::Playing, 1_500, 9_000)
    );
}

#[test]
fn reset_to_idle_clears_position_tracks_and_state() {
    let tracker = playing_tracker();
    tracker.update_progress(fresh(&tracker, 1_500, 9_000, true));
    tracker.reset_to_idle();
    assert_eq!(tracker.snapshot(), PlaybackStatus::default());
    assert!(tracker.tracks().is_empty());
    assert!(tracker.chapters().is_empty());
}

#[test]
fn progress_is_ignored_when_nothing_is_playing() {
    let tracker = StatusTracker::default();
    tracker.update_progress(fresh(&tracker, 5, 5, false));
    assert_eq!(tracker.snapshot(), PlaybackStatus::default());
}

#[test]
fn pause_flag_switches_between_playing_and_paused() {
    let tracker = playing_tracker();
    tracker.update_progress(fresh(&tracker, 1, 2, true));
    assert_eq!(tracker.snapshot().state, PlaybackState::Paused);
    tracker.update_progress(fresh(&tracker, 1, 2, false));
    assert_eq!(tracker.snapshot().state, PlaybackState::Playing);
}

#[test]
fn buffering_and_buffered_position_are_tracked() {
    let tracker = playing_tracker();
    let mut reading = fresh(&tracker, 1_000, 10_000, false);
    reading.buffered_ms = 4_000;
    reading.is_buffering = true;
    reading.buffer_percent = Some(40);
    tracker.update_progress(reading);
    let snapshot = tracker.snapshot();
    assert_eq!(snapshot.buffered_ms, 4_000);
    assert!(snapshot.is_buffering);
    assert_eq!(snapshot.buffer_percent, Some(40));
}

#[test]
fn end_events_map_to_ended_idle_and_failed() {
    let tracker = StatusTracker::default();
    tracker.apply(MpvEvent::EndFile(EndReason::Finished));
    assert_eq!(tracker.snapshot().state, PlaybackState::Ended);
    tracker.apply(MpvEvent::EndFile(EndReason::Stopped));
    assert_eq!(tracker.snapshot().state, PlaybackState::Idle);
    tracker.apply(MpvEvent::EndFile(EndReason::Failed(-13)));
    assert!(matches!(tracker.snapshot().state, PlaybackState::Failed(_)));
}

#[test]
fn a_pause_shows_at_once_and_keeps_the_position() {
    let tracker = playing_tracker();
    tracker.update_progress(fresh(&tracker, 4_000, 9_000, false));
    tracker.set_paused(true);
    let snapshot = tracker.snapshot();
    assert_eq!(
        (snapshot.state, snapshot.position_ms),
        (PlaybackState::Paused, 4_000)
    );
    tracker.set_paused(false);
    assert_eq!(tracker.snapshot().state, PlaybackState::Playing);
}

#[test]
fn a_pause_does_not_turn_loading_ended_or_idle_into_paused() {
    let tracker = StatusTracker::default();
    tracker.set_paused(true);
    assert_eq!(tracker.snapshot().state, PlaybackState::Idle);
    tracker.begin_loading();
    tracker.set_paused(true);
    assert_eq!(tracker.snapshot().state, PlaybackState::Loading);
    tracker.apply(MpvEvent::EndFile(EndReason::Finished));
    tracker.set_paused(false);
    assert_eq!(tracker.snapshot().state, PlaybackState::Ended);
}

/// The interleaving that used to show "Playing" again right after a pause: the event thread reads
/// mpv (still playing), the viewer pauses, and only then the event thread stores what it read.
#[test]
fn a_reading_taken_before_a_pause_does_not_undo_it() {
    let tracker = playing_tracker();
    let stale = fresh(&tracker, 4_000, 9_000, false);
    tracker.set_paused(true);
    tracker.update_progress(stale);
    assert_eq!(tracker.snapshot().state, PlaybackState::Paused);
    // The next reading is taken after the pause and applies as usual.
    tracker.update_progress(fresh(&tracker, 4_100, 9_000, true));
    let snapshot = tracker.snapshot();
    assert_eq!(
        (snapshot.state, snapshot.position_ms),
        (PlaybackState::Paused, 4_100)
    );
}

/// Without the command count the same stale reading wins: this is what the guard prevents.
#[test]
fn the_same_stale_reading_would_undo_the_pause_without_the_guard() {
    let tracker = playing_tracker();
    let stale = fresh(&tracker, 4_000, 9_000, false);
    tracker.set_paused(true);
    tracker.update_progress(Reading {
        pause_commands: tracker.pause_commands(), // pretends the reading is newer than it is
        ..stale
    });
    assert_eq!(tracker.snapshot().state, PlaybackState::Playing);
}

/// Stands in for mpv and the event thread: reads the pause flag and stores it, as fast as it can.
fn event_thread(tracker: &StatusTracker, mpv_paused: &AtomicBool, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        let pause_commands = tracker.pause_commands();
        let paused = mpv_paused.load(Ordering::SeqCst);
        std::thread::yield_now(); // widen the gap between reading mpv and storing the result
        tracker.update_progress(Reading {
            position_ms: 1,
            duration_ms: 2,
            paused,
            pause_commands,
            ..Reading::default()
        });
    }
}

#[test]
fn the_status_always_shows_the_last_pause_while_the_event_thread_keeps_reading() {
    let tracker = Arc::new(playing_tracker());
    let mpv_paused = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let (tracker, mpv_paused, stop) = (
                Arc::clone(&tracker),
                Arc::clone(&mpv_paused),
                Arc::clone(&stop),
            );
            std::thread::spawn(move || event_thread(&tracker, &mpv_paused, &stop))
        })
        .collect();
    let mut wrong = 0_u32;
    for round in 0..20_000_u32 {
        let paused = round % 2 == 0;
        // As `MpvPlayer::set_paused` does: mpv first, then the status.
        mpv_paused.store(paused, Ordering::SeqCst);
        tracker.set_paused(paused);
        std::thread::yield_now();
        let shown = tracker.snapshot().state == PlaybackState::Paused;
        wrong += u32::from(shown != paused);
    }
    stop.store(true, Ordering::Relaxed);
    for reader in readers {
        reader.join().expect("reader finished");
    }
    assert_eq!(wrong, 0, "the status showed the state from before a pause");
}
