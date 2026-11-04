use std::sync::{Mutex, MutexGuard, PoisonError};

use player_api::{Chapter, MediaTrack, PlaybackState, PlaybackStatus};

use crate::mpv_event::{EndReason, MpvEvent};

/// Shared playback snapshot updated by the event thread and read by the UI.
#[derive(Default)]
pub(crate) struct StatusTracker {
    status: Mutex<PlaybackStatus>,
    tracks: Mutex<Vec<MediaTrack>>,
    chapters: Mutex<Vec<Chapter>>,
}

impl StatusTracker {
    fn lock(&self) -> MutexGuard<'_, PlaybackStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns a copy of the current snapshot.
    pub(crate) fn snapshot(&self) -> PlaybackStatus {
        self.lock().clone()
    }

    /// The latest track list.
    pub(crate) fn tracks(&self) -> Vec<MediaTrack> {
        self.tracks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The chapters of the loaded stream.
    pub(crate) fn chapters(&self) -> Vec<Chapter> {
        self.chapters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replaces the chapter list.
    pub(crate) fn set_chapters(&self, chapters: Vec<Chapter>) {
        *self.chapters.lock().unwrap_or_else(PoisonError::into_inner) = chapters;
    }

    /// Records whether the loaded stream has a picture.
    pub(crate) fn set_has_video(&self, has_video: bool) {
        self.lock().has_video = has_video;
    }

    /// Replaces the track list.
    pub(crate) fn set_tracks(&self, tracks: Vec<MediaTrack>) {
        *self.tracks.lock().unwrap_or_else(PoisonError::into_inner) = tracks;
    }

    /// Marks a new stream as opening and clears the old position.
    pub(crate) fn begin_loading(&self) {
        self.set_tracks(Vec::new());
        self.set_chapters(Vec::new());
        *self.lock() = PlaybackStatus {
            state: PlaybackState::Loading,
            // Assume a picture until the track list says otherwise, so nothing flickers.
            has_video: true,
            ..PlaybackStatus::default()
        };
    }

    /// Returns to the idle state at once, forgetting position, tracks and picture.
    pub(crate) fn reset_to_idle(&self) {
        self.set_tracks(Vec::new());
        self.set_chapters(Vec::new());
        *self.lock() = PlaybackStatus::default();
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
                tracing::warn!(code, "mpv could not play the stream");
                status.state = PlaybackState::Failed(format!("mpv error {code}"));
            }
            MpvEvent::Shutdown => status.state = PlaybackState::Idle,
            MpvEvent::Nothing | MpvEvent::Other => return,
        }
        tracing::debug!(?event, state = ?status.state, "player event applied");
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
    fn reset_to_idle_clears_position_tracks_and_state() {
        let tracker = StatusTracker::default();
        tracker.apply(MpvEvent::FileLoaded);
        tracker.update_progress(1_500, 9_000, true);
        tracker.reset_to_idle();
        assert_eq!(tracker.snapshot(), PlaybackStatus::default());
        assert!(tracker.tracks().is_empty());
        assert!(tracker.chapters().is_empty());
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
