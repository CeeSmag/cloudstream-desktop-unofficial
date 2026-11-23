use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use player_api::{
    CacheLimits, Chapter, GlVideoTarget, Language, MediaTrack, PlaybackRequest, PlaybackStatus,
    PlayerError, SubtitleSource, TrackKind, VideoFrame, VideoPlayer,
};

/// What the fake player answers when asked about the stream.
#[derive(Default, Clone)]
pub(crate) struct PlayerAnswers {
    pub(crate) tracks: Vec<MediaTrack>,
    pub(crate) chapters: Vec<Chapter>,
    pub(crate) pending_subtitles: usize,
    pub(crate) embedded: bool,
    pub(crate) frame: Option<Arc<VideoFrame>>,
    pub(crate) gl_target: Option<Arc<dyn GlVideoTarget>>,
}

/// A player that records requests and reports a scripted status.
#[derive(Default)]
pub(crate) struct FakePlayer {
    pub(crate) requests: Mutex<Vec<PlaybackRequest>>,
    status: Mutex<PlaybackStatus>,
    pub(crate) verbose_log: AtomicBool,
    /// Every control call, in order, as `paused true` / `cache 3 1` / `seek 5000` ...
    pub(crate) calls: Mutex<Vec<String>>,
    /// When set, every control that can fail answers with this error.
    pub(crate) failing: Mutex<Option<String>>,
    /// The answers to questions about the stream.
    pub(crate) answers: Mutex<PlayerAnswers>,
}

impl FakePlayer {
    pub(crate) fn report(&self, status: PlaybackStatus) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }

    fn answers(&self) -> PlayerAnswers {
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn note(&self, call: String) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }

    /// Notes `call` and fails it when the player was told to fail.
    fn command(&self, call: String) -> Result<(), PlayerError> {
        let failing = self
            .failing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        self.note(call.clone());
        match failing {
            Some(reason) => Err(PlayerError::Command {
                command: call,
                reason,
            }),
            None => Ok(()),
        }
    }
}

impl VideoPlayer for FakePlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        Ok(())
    }

    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        self.command(format!("paused {paused}"))
    }

    fn set_cache_limits(&self, limits: CacheLimits) -> Result<(), PlayerError> {
        self.command(format!(
            "cache {} {}",
            limits.ahead_bytes, limits.behind_bytes
        ))
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        self.command(format!("seek {position_ms}"))
    }

    fn stop(&self) -> Result<(), PlayerError> {
        self.command("stop".to_owned())
    }

    fn set_volume(&self, percent: u8) -> Result<(), PlayerError> {
        self.command(format!("volume {percent}"))
    }

    fn select_track(&self, kind: TrackKind, id: Option<i64>) -> Result<(), PlayerError> {
        self.command(format!("track {kind:?} {id:?}"))
    }

    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        for subtitle in subtitles {
            self.note(format!("subtitle {} {}", subtitle.language, subtitle.url));
        }
    }

    fn set_preferred_audio(&self, language: Option<Language>) {
        self.note(format!("audio {:?}", language.map(|l| l.name)));
    }

    fn set_preferred_subtitles(&self, languages: &[Language]) {
        let names: Vec<&str> = languages.iter().map(|l| l.name).collect();
        self.note(format!("subtitles {names:?}"));
    }

    fn set_surface_size(&self, width: u32, height: u32) {
        self.note(format!("surface {width}x{height}"));
    }

    fn tracks(&self) -> Vec<MediaTrack> {
        self.answers().tracks
    }

    fn chapters(&self) -> Vec<Chapter> {
        self.answers().chapters
    }

    fn pending_subtitles(&self) -> usize {
        self.answers().pending_subtitles
    }

    fn is_embedded(&self) -> bool {
        self.answers().embedded
    }

    fn latest_frame(&self, last_seen_serial: u64) -> Option<Arc<VideoFrame>> {
        self.answers()
            .frame
            .filter(|frame| frame.serial != last_seen_serial)
    }

    fn gl_target(&self) -> Option<Arc<dyn GlVideoTarget>> {
        self.answers().gl_target
    }

    fn set_verbose_log(&self, enabled: bool) {
        self.verbose_log.store(enabled, Ordering::SeqCst);
    }

    fn status(&self) -> PlaybackStatus {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
