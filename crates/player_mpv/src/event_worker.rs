use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use player_api::{MediaTrack, TrackKind};

use crate::load_tracker::LoadTracker;
use crate::mpv_api::MpvHandle;
use crate::mpv_event::MpvEvent;
use crate::pending_subtitles::{take_when_loaded, PendingSubtitles};
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
    pub(crate) pending_subtitles: Arc<Mutex<PendingSubtitles>>,
    pub(crate) stop_flag: Arc<AtomicBool>,
    pub(crate) stream_generation: StreamGeneration,
    pub(crate) audio_preference: Arc<AudioPreference>,
    /// How many subtitle tracks the track list showed the last time it was logged.
    pub(crate) reported_subtitle_tracks: AtomicUsize,
}

impl EventWorker {
    pub(crate) fn run(self) {
        let mut ticks: u32 = 0;
        let mut loads = LoadTracker::default();
        while !self.stop_flag.load(Ordering::Relaxed) {
            let event = self.handle.wait_event(EVENT_WAIT);
            self.tracker.apply(event);
            let generation = self.stream_generation.load(Ordering::SeqCst);
            loads.apply(event, generation);
            if event == MpvEvent::FileLoaded {
                tracing::info!(
                    generation,
                    waiting_subtitle_files = self.waiting_subtitles(),
                    "stream loaded"
                );
                self.attach_subtitles(loads.loaded());
                self.refresh_tracks();
                self.apply_audio_preference();
            } else if loads.loaded().is_some() {
                // Subtitles the provider found after the stream started.
                self.attach_subtitles(loads.loaded());
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

    fn waiting_subtitles(&self) -> usize {
        self.pending_subtitles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Hands the stream's external subtitles to background workers; nothing here waits on a download.
    #[tracing::instrument(level = "debug", skip_all)]
    fn attach_subtitles(&self, loaded: Option<u64>) {
        let Some((generation, subtitles)) =
            take_when_loaded(&self.pending_subtitles, &self.stream_generation, loaded)
        else {
            return;
        };
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
        let tracks = read_tracks(get);
        self.report_subtitle_tracks(&tracks);
        self.tracker.set_tracks(tracks);
        self.tracker.set_has_video(has_video_track(get));
    }

    /// Logs the subtitle tracks the player lists whenever their number changes, so a language
    /// that never shows up can be told from one that was attached and then lost.
    fn report_subtitle_tracks(&self, tracks: &[MediaTrack]) {
        let languages: Vec<&str> = tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Subtitle)
            .map(|track| track.language.as_str())
            .collect();
        if self
            .reported_subtitle_tracks
            .swap(languages.len(), Ordering::Relaxed)
            == languages.len()
        {
            return;
        }
        let mut counts = std::collections::BTreeMap::new();
        for language in &languages {
            *counts
                .entry(if language.is_empty() {
                    "unknown"
                } else {
                    language
                })
                .or_insert(0usize) += 1;
        }
        let tally: Vec<String> = counts
            .iter()
            .map(|(language, count)| format!("{language}={count}"))
            .collect();
        tracing::info!(
            subtitle_tracks = languages.len(),
            languages = %tally.join(", "),
            "the subtitle tracks in the player changed"
        );
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
