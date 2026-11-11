use std::sync::{Mutex, MutexGuard, PoisonError};

use player_api::{Chapter, MediaTrack, PlaybackState, PlaybackStatus};

use crate::mpv_event::{EndReason, MpvEvent};

/// What the event thread read from mpv in one go.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Reading {
    pub(crate) position_ms: i64,
    pub(crate) duration_ms: i64,
    pub(crate) paused: bool,
    pub(crate) buffered_ms: i64,
    pub(crate) is_buffering: bool,
    pub(crate) buffer_percent: Option<u32>,
    /// [`StatusTracker::pause_commands`] taken before mpv was read. A pause or play given since
    /// then makes the reading out of date.
    pub(crate) pause_commands: u64,
}

/// The snapshot with the number of pause/play commands it has seen; they change together.
#[derive(Default)]
struct Tracked {
    status: PlaybackStatus,
    pause_commands: u64,
}

/// Shared playback snapshot updated by the event thread and read by the UI.
#[derive(Default)]
pub(crate) struct StatusTracker {
    status: Mutex<Tracked>,
    tracks: Mutex<Vec<MediaTrack>>,
    chapters: Mutex<Vec<Chapter>>,
}

impl StatusTracker {
    fn lock(&self) -> MutexGuard<'_, Tracked> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns a copy of the current snapshot.
    pub(crate) fn snapshot(&self) -> PlaybackStatus {
        self.lock().status.clone()
    }

    /// How many pause/play commands the viewer has given; see [`Reading::pause_commands`].
    pub(crate) fn pause_commands(&self) -> u64 {
        self.lock().pause_commands
    }

    /// Shows a pause or play the moment mpv has accepted it, instead of at the event thread's
    /// next reading. Call it after mpv has the new value.
    pub(crate) fn set_paused(&self, paused: bool) {
        let mut tracked = self.lock();
        tracked.pause_commands += 1;
        if matches!(
            tracked.status.state,
            PlaybackState::Playing | PlaybackState::Paused
        ) {
            tracked.status.state = paused_state(paused);
        }
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
        self.lock().status.has_video = has_video;
    }

    /// Replaces the track list.
    pub(crate) fn set_tracks(&self, tracks: Vec<MediaTrack>) {
        *self.tracks.lock().unwrap_or_else(PoisonError::into_inner) = tracks;
    }

    /// Marks a new stream as opening and clears the old position.
    pub(crate) fn begin_loading(&self) {
        self.set_tracks(Vec::new());
        self.set_chapters(Vec::new());
        self.lock().status = PlaybackStatus {
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
        self.lock().status = PlaybackStatus::default();
    }

    /// Applies an mpv event to the snapshot.
    pub(crate) fn apply(&self, event: MpvEvent) {
        let mut tracked = self.lock();
        let status = &mut tracked.status;
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

    /// Records the latest position/length and pause flag while a stream is active. A reading taken
    /// before the viewer's last pause or play is dropped: its pause flag may be the old one, and
    /// the next reading is at most a moment away.
    pub(crate) fn update_progress(&self, reading: Reading) {
        let mut tracked = self.lock();
        if reading.pause_commands != tracked.pause_commands {
            return;
        }
        let status = &mut tracked.status;
        if !matches!(status.state, PlaybackState::Playing | PlaybackState::Paused) {
            return;
        }
        status.position_ms = reading.position_ms;
        status.duration_ms = reading.duration_ms;
        status.buffered_ms = reading.buffered_ms;
        status.is_buffering = reading.is_buffering;
        status.buffer_percent = reading.buffer_percent;
        status.state = paused_state(reading.paused);
    }

    /// Replaces a generic failure with a more specific message.
    pub(crate) fn fail(&self, reason: String) {
        self.lock().status.state = PlaybackState::Failed(reason);
    }
}

fn paused_state(paused: bool) -> PlaybackState {
    if paused {
        PlaybackState::Paused
    } else {
        PlaybackState::Playing
    }
}

#[cfg(test)]
#[path = "status_tracker_tests.rs"]
mod tests;
