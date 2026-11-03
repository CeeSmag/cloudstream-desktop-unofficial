//! A player for tests that records what it was asked to play and can be told to fail.

use std::sync::Mutex;
use std::time::Duration;

use player_api::{
    PlaybackRequest, PlaybackState, PlaybackStatus, PlayerError, SubtitleSource, VideoPlayer,
};

/// Records what the player was asked to play.
#[derive(Default)]
pub(crate) struct RecordingPlayer {
    pub(crate) played: Mutex<Vec<String>>,
    /// Where each stream was asked to begin, in milliseconds.
    pub(crate) positions: Mutex<Vec<i64>>,
    /// The subtitle files each stream was started with.
    pub(crate) started_with: Mutex<Vec<Vec<SubtitleSource>>>,
    /// Subtitle files given to a stream that was already playing.
    pub(crate) added_later: Mutex<Vec<SubtitleSource>>,
    /// Streams whose address holds one of these words are refused when started.
    pub(crate) refuses: Mutex<Vec<String>>,
    /// How long starting a stream takes.
    pub(crate) start_takes: Mutex<Duration>,
}

impl VideoPlayer for RecordingPlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        let pause = *self.start_takes.lock().expect("lock");
        std::thread::sleep(pause);
        let refused = self
            .refuses
            .lock()
            .expect("lock")
            .iter()
            .any(|word| request.url.contains(word));
        if refused {
            return Err(PlayerError::Command {
                command: "loadfile".to_owned(),
                reason: "refused in this test".to_owned(),
            });
        }
        self.played.lock().expect("lock").push(request.url.clone());
        self.positions
            .lock()
            .expect("lock")
            .push(request.start_position_ms);
        self.started_with
            .lock()
            .expect("lock")
            .push(request.subtitles.clone());
        Ok(())
    }
    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        self.added_later
            .lock()
            .expect("lock")
            .extend_from_slice(subtitles);
    }
    fn set_paused(&self, _: bool) -> Result<(), PlayerError> {
        Ok(())
    }
    fn seek_to(&self, _: i64) -> Result<(), PlayerError> {
        Ok(())
    }
    fn stop(&self) -> Result<(), PlayerError> {
        Ok(())
    }
    fn status(&self) -> PlaybackStatus {
        PlaybackStatus::default()
    }
}

/// What the player reports when the stream failed.
pub(crate) fn failed(reason: &str) -> PlaybackStatus {
    PlaybackStatus {
        state: PlaybackState::Failed(reason.to_owned()),
        ..PlaybackStatus::default()
    }
}

/// What the player reports while a stream plays.
pub(crate) fn playing() -> PlaybackStatus {
    PlaybackStatus {
        state: PlaybackState::Playing,
        position_ms: 1_000,
        ..PlaybackStatus::default()
    }
}
