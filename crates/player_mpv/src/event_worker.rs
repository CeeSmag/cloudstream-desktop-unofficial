use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use player_api::{MediaTrack, TrackKind};

use crate::chapter_reader::read_chapters;
use crate::load_tracker::LoadTracker;
use crate::mpv_api::MpvHandle;
use crate::mpv_event::MpvEvent;
use crate::pending_subtitles::{take_when_loaded, PendingSubtitles};
use crate::preferred_audio::{preferred_audio_choice, AudioChoice, AudioPreference};
use crate::preferred_subtitles::{preferred_subtitle_choice, SubtitleChoice, SubtitlePreference};
use crate::status_tracker::{Reading, StatusTracker};
use crate::subtitle_attacher::{attach_in_background, StreamGeneration};
use crate::track_reader::{has_video_track, read_tracks};

/// How long the event thread waits per iteration; also its stop-latency and how old the position
/// the app shows can get.
const EVENT_WAIT: Duration = Duration::from_millis(100);

/// The track list is re-read every this many event iterations (about once a second).
const TRACK_REFRESH_EVERY: u32 = 10;

/// Background loop that turns mpv events and property polls into status updates.
pub(crate) struct EventWorker {
    pub(crate) handle: Arc<MpvHandle>,
    pub(crate) tracker: Arc<StatusTracker>,
    pub(crate) pending_subtitles: Arc<Mutex<PendingSubtitles>>,
    pub(crate) stop_flag: Arc<AtomicBool>,
    pub(crate) stream_generation: StreamGeneration,
    pub(crate) audio_preference: Arc<AudioPreference>,
    pub(crate) subtitle_preference: Arc<SubtitlePreference>,
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
                self.refresh_chapters();
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
                self.refresh_chapters();
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

    /// Reads the chapters the stream carries (most web streams have none).
    pub(crate) fn refresh_chapters(&self) {
        let chapters = read_chapters(|name| self.handle.get_string(name));
        if chapters != self.tracker.chapters() {
            tracing::info!(chapters = chapters.len(), "the stream's chapters changed");
            self.tracker.set_chapters(chapters);
        }
    }

    /// Re-reads the track list (subtitles added after loading show up here too).
    pub(crate) fn refresh_tracks(&self) {
        let get = |name: &str| self.handle.get_string(name);
        let mut tracks = read_tracks(get);
        if self.apply_subtitle_preference(&tracks) {
            tracks = read_tracks(get);
        }
        self.report_subtitle_tracks(&tracks);
        self.tracker.set_tracks(tracks);
        self.tracker.set_has_video(has_video_track(get));
    }

    /// Shows the subtitle in the viewer's preferred language, once per stream, as soon as one is
    /// there and unless the viewer has chosen by hand. Returns whether a track was switched.
    fn apply_subtitle_preference(&self, tracks: &[MediaTrack]) -> bool {
        let generation = self.stream_generation.load(Ordering::SeqCst);
        if !self.subtitle_preference.may_choose(generation) {
            return false;
        }
        match preferred_subtitle_choice(tracks, &self.subtitle_preference.languages()) {
            SubtitleChoice::NotYet => false,
            SubtitleChoice::Keep => {
                self.subtitle_preference.mark_chosen(generation);
                false
            }
            SubtitleChoice::Switch { id } => {
                self.subtitle_preference.mark_chosen(generation);
                match self.handle.set_property("sid", &id.to_string()) {
                    Ok(()) => {
                        tracing::info!(id, "showing the subtitle in the preferred language");
                        true
                    }
                    Err(error) => {
                        tracing::warn!(%error, id, "could not show the subtitle in the preferred language");
                        false
                    }
                }
            }
        }
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
        // Taken before mpv is read: a pause given while reading makes this reading out of date.
        let pause_commands = self.tracker.pause_commands();
        let Some(position) = self.handle.get_double("time-pos") else {
            return;
        };
        // Live streams (infinite HLS or dynamic DASH) often have no finite `duration` property in
        // mpv; defaulting to 0.0 keeps `position_ms` and the `pause` state updating normally.
        let duration = self
            .handle
            .get_double("duration")
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(0.0);
        let paused = self.handle.get_flag("pause").unwrap_or(false);
        let paused_for_cache = self.handle.get_flag("paused-for-cache").unwrap_or(false);
        let cache_buffering = self.handle.get_double("cache-buffering-state");
        let is_buffering =
            paused_for_cache || cache_buffering.is_some_and(|pct| pct < 100.0 && !paused);
        let buffer_percent = cache_buffering.map(|pct| pct.clamp(0.0, 100.0) as u32);
        let cache_sec = self
            .handle
            .get_double("demuxer-cache-duration")
            .or_else(|| self.handle.get_double("demuxer-cache-time"))
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(0.0);
        let position_ms = (position.max(0.0) * 1000.0) as i64;
        let duration_ms = (duration * 1000.0) as i64;
        let mut buffered_ms = position_ms + (cache_sec * 1000.0) as i64;
        if duration_ms > 0 && buffered_ms > duration_ms {
            buffered_ms = duration_ms;
        }
        self.tracker.update_progress(Reading {
            position_ms,
            duration_ms,
            paused,
            buffered_ms,
            is_buffering,
            buffer_percent,
            pause_commands,
        });
    }
}
