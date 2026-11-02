use std::sync::{Mutex, MutexGuard, PoisonError};

use player_api::{PlaybackState, PlaybackStatus};

use crate::mpv_event::{EndReason, MpvEvent};

/// Shared playback snapshot updated by the event thread and read by the UI.
#[derive(Default)]
pub(crate) struct StatusTracker {
    status: Mutex<PlaybackStatus>,
}

impl StatusTracker {
    fn lock(&self) -> MutexGuard<'_, PlaybackStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns a copy of the current snapshot.
    pub(crate) fn snapshot(&self) -> PlaybackStatus {
        self.lock().clone()
    }

    /// Marks a new stream as opening and clears the old position.
    pub(crate) fn begin_loading(&self) {
        *self.lock() = PlaybackStatus {
            state: PlaybackState::Loading,
            ..PlaybackStatus::default()
        };
    }

    /// Applies an mpv event to the snapshot.
    pub(crate) fn apply(&self, event: MpvEvent) {
        let mut status = self.lock();
        match event {
            MpvEvent::StartFile => status.state = PlaybackState::Loading,
            MpvEvent::FileLoaded => status.state = PlaybackState::Playing,
            MpvEvent::EndFile(EndReason::Finished) => status.state = PlaybackState::Ended,
            MpvEvent::EndFile(EndReason::Stopped) => status.state = PlaybackState::Idle,
            MpvEvent::EndFile(EndReason::Failed(code)) => {
                status.state = PlaybackState::Failed(format!("mpv error {code}"));
            }
            MpvEvent::Shutdown => status.state = PlaybackState::Idle,
            MpvEvent::Nothing | MpvEvent::Other => {}
        }
    }

    /// Records the latest position/length and pause flag while a stream is active.
    pub(crate) fn update_progress(&self, position_ms: i64, duration_ms: i64, paused: bool) {
        let mut status = self.lock();
        if !matches!(status.state, PlaybackState::Playing | PlaybackState::Paused) {
            return;
        }
        status.position_ms = position_ms;
        status.duration_ms = duration_ms;
        status.state = if paused {
            PlaybackState::Paused
        } else {
            PlaybackState::Playing
        };
    }

    /// Replaces a generic failure with a more specific message.
    pub(crate) fn fail(&self, reason: String) {
        self.lock().state = PlaybackState::Failed(reason);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_loaded_moves_loading_to_playing_and_progress_updates_apply() {
        let tracker = StatusTracker::default();
        tracker.begin_loading();
        assert_eq!(tracker.snapshot().state, PlaybackState::Loading);
        tracker.apply(MpvEvent::FileLoaded);
        tracker.update_progress(1_500, 9_000, false);
        let snapshot = tracker.snapshot();
        assert_eq!(
            (snapshot.state, snapshot.position_ms, snapshot.duration_ms),
            (PlaybackState::Playing, 1_500, 9_000)
        );
    }

    #[test]
    fn progress_is_ignored_when_nothing_is_playing() {
        let tracker = StatusTracker::default();
        tracker.update_progress(5, 5, false);
        assert_eq!(tracker.snapshot(), PlaybackStatus::default());
    }

    #[test]
    fn pause_flag_switches_between_playing_and_paused() {
        let tracker = StatusTracker::default();
        tracker.apply(MpvEvent::FileLoaded);
        tracker.update_progress(1, 2, true);
        assert_eq!(tracker.snapshot().state, PlaybackState::Paused);
        tracker.update_progress(1, 2, false);
        assert_eq!(tracker.snapshot().state, PlaybackState::Playing);
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
}
