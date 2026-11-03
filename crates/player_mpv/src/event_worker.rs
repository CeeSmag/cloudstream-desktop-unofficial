use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use player_api::{MediaTrack, SubtitleSource, TrackKind};

use crate::mpv_api::MpvHandle;
use crate::mpv_event::MpvEvent;
use crate::preferred_audio::{preferred_audio_choice, AudioChoice, AudioPreference};
use crate::status_tracker::StatusTracker;
use crate::subtitle_attacher::{attach_in_background, StreamGeneration};
use crate::track_reader::{has_video_track, read_tracks};

/// How long the event thread waits per iteration; also its stop-latency.
const EVENT_WAIT: Duration = Duration::from_millis(200);

/// The track list is re-read every this many event iterations (about once a second).
const TRACK_REFRESH_EVERY: u32 = 5;

/// Background loop that turns mpv events and property polls into status updates.
pub(crate) struct EventWorker {
    pub(crate) handle: Arc<MpvHandle>,
    pub(crate) tracker: Arc<StatusTracker>,
    pub(crate) pending_subtitles: Arc<Mutex<Vec<SubtitleSource>>>,
    pub(crate) stop_flag: Arc<AtomicBool>,
    pub(crate) stream_generation: StreamGeneration,
    pub(crate) audio_preference: Arc<AudioPreference>,
}

impl EventWorker {
    pub(crate) fn run(self) {
        let mut ticks: u32 = 0;
        let mut loaded = false;
        while !self.stop_flag.load(Ordering::Relaxed) {
            let event = self.handle.wait_event(EVENT_WAIT);
            self.tracker.apply(event);
            loaded = match event {
                MpvEvent::FileLoaded => true,
                MpvEvent::StartFile | MpvEvent::EndFile(_) => false,
                _ => loaded,
            };
            if event == MpvEvent::FileLoaded {
                self.attach_subtitles();
                self.refresh_tracks();
                self.apply_audio_preference();
            } else if loaded {
                // Subtitles the provider found after the stream started.
                self.attach_subtitles();
            }
            if event == MpvEvent::Shutdown {
                break;
            }
            self.poll_progress();
            ticks += 1;
            if ticks.is_multiple_of(TRACK_REFRESH_EVERY) {
                self.refresh_tracks();
            }
        }
    }

    /// Hands the stream's external subtitles to background workers; nothing here waits on a download.
    #[tracing::instrument(level = "debug", skip_all)]
    fn attach_subtitles(&self) {
        let subtitles = std::mem::take(
            &mut *self
                .pending_subtitles
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        if subtitles.is_empty() {
            return;
        }
        let generation = self.stream_generation.load(Ordering::SeqCst);
        attach_in_background(&self.handle, &self.stream_generation, generation, subtitles);
    }

    /// Plays the audio track in the viewer's preferred language when the stream has one.
    /// Whatever goes wrong here, the stream keeps playing on the track it started with.
    fn apply_audio_preference(&self) {
        let Some(language) = self.audio_preference.get() else {
            return;
        };
        let tracks = self.tracker.tracks();
        match preferred_audio_choice(&tracks, &language) {
            AudioChoice::Keep => {}
            AudioChoice::Switch { id } => match self.handle.set_property("aid", &id.to_string()) {
                Ok(()) => {
                    tracing::info!(
                        id,
                        language = language.name,
                        "playing the preferred audio track"
                    );
                    self.refresh_tracks();
                }
                Err(error) => {
                    tracing::warn!(%error, id, language = language.name, "could not switch to the preferred audio track; keeping the stream's default");
                }
            },
            AudioChoice::NotOffered => {
                let playing = tracks
                    .iter()
                    .find(|track| track.selected && track.kind == TrackKind::Audio)
                    .map(MediaTrack::label);
                tracing::info!(
                    ?playing,
                    language = language.name,
                    "the stream has no audio track in the preferred language; playing its default"
                );
            }
        }
    }

    /// Re-reads the track list (subtitles added after loading show up here too).
    pub(crate) fn refresh_tracks(&self) {
        let get = |name: &str| self.handle.get_string(name);
        self.tracker.set_tracks(read_tracks(get));
        self.tracker.set_has_video(has_video_track(get));
    }

    fn poll_progress(&self) {
        let (Some(position), Some(duration)) = (
            self.handle.get_double("time-pos"),
            self.handle.get_double("duration"),
        ) else {
            return;
        };
        let paused = self.handle.get_flag("pause").unwrap_or(false);
        self.tracker.update_progress(
            (position * 1000.0) as i64,
            (duration * 1000.0) as i64,
            paused,
        );
    }
}
