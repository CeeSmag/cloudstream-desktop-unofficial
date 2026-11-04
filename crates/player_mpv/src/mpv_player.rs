use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use player_api::{
    describe_languages, Language, MediaTrack, PlaybackRequest, PlaybackStatus, PlayerError,
    SubtitleSource, TrackKind, VideoFrame, VideoPlayer,
};

use crate::event_worker::EventWorker;
use crate::frame_renderer::{FrameRenderer, FrameShared};
use crate::mpv_api::{MpvApi, MpvHandle};
use crate::mpv_log::log_level;
use crate::mpv_option_format::{header_fields, start_option};
use crate::mpv_render_api::{FrameSignal, MpvRenderContext};
use crate::pending_subtitles::{queue_for_new_stream, PendingSubtitles};
use crate::player_configuration::configure;
pub use crate::player_configuration::PlayerOptions;
use crate::preferred_audio::{alang_value, AudioPreference};
use crate::preferred_subtitles::SubtitlePreference;
use crate::status_tracker::StatusTracker;
use crate::track_reader::read_tracks;

/// Highest volume accepted; mpv itself allows amplifying up to 130.
const MAX_VOLUME_PERCENT: u8 = 130;

/// Size used for the first frames, before the UI reports its real picture area.
const INITIAL_SURFACE: (u32, u32) = (1280, 720);

/// `VideoPlayer` backed by libmpv: embedded (pictures handed to the app) or in mpv's own window.
pub struct MpvPlayer {
    tracker: Arc<StatusTracker>,
    pending_subtitles: Arc<Mutex<PendingSubtitles>>,
    stop_flag: Arc<AtomicBool>,
    stream_generation: Arc<AtomicU64>,
    audio_preference: Arc<AudioPreference>,
    subtitle_preference: Arc<SubtitlePreference>,
    event_thread: Option<JoinHandle<()>>,
    frames: Option<Arc<FrameShared>>,
    // Dropped before `handle` (declaration order) so the render context never outlives mpv.
    _renderer: Option<FrameRenderer>,
    handle: Arc<MpvHandle>,
}

impl MpvPlayer {
    #[tracing::instrument(level = "debug", skip_all)]
    fn unpause_before_new_stream(&self) {
        if let Err(error) = self.handle.set_property("pause", "no") {
            tracing::warn!(%error, "could not clear the pause flag");
        }
    }

    /// Loads libmpv from `library_path` and starts an idle player.
    #[tracing::instrument(level = "debug", skip_all, fields(library_path = ?library_path), err)]
    pub fn new(library_path: &Path, options: &PlayerOptions) -> Result<Self, PlayerError> {
        let api = Arc::new(MpvApi::load(library_path)?);
        let embedded = wants_embedding(&api, options);
        let handle = Arc::new(MpvHandle::create(api)?);
        configure(&handle, options, embedded)?;
        handle.initialize()?;
        handle.set_log_level(log_level(false));
        let (renderer, frames) = if embedded {
            start_embedding(&handle)
        } else {
            (None, None)
        };
        let tracker = Arc::new(StatusTracker::default());
        let pending_subtitles = Arc::new(Mutex::new(PendingSubtitles::default()));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stream_generation = Arc::new(AtomicU64::new(0));
        let audio_preference = Arc::new(AudioPreference::default());
        let subtitle_preference = Arc::new(SubtitlePreference::default());
        let event_thread = start_event_thread(EventWorker {
            handle: Arc::clone(&handle),
            tracker: Arc::clone(&tracker),
            pending_subtitles: Arc::clone(&pending_subtitles),
            stop_flag: Arc::clone(&stop_flag),
            stream_generation: Arc::clone(&stream_generation),
            audio_preference: Arc::clone(&audio_preference),
            subtitle_preference: Arc::clone(&subtitle_preference),
            reported_subtitle_tracks: std::sync::atomic::AtomicUsize::new(0),
        })?;
        Ok(Self {
            tracker,
            pending_subtitles,
            stop_flag,
            stream_generation,
            audio_preference,
            subtitle_preference,
            event_thread: Some(event_thread),
            frames,
            _renderer: renderer,
            handle,
        })
    }
}

/// Whether frames are rendered into memory for the app window rather than in mpv's own window.
fn wants_embedding(api: &MpvApi, options: &PlayerOptions) -> bool {
    let embedded = options.embedded
        && !options.headless
        && options.wid.is_none()
        && api.render_api().is_some();
    if options.embedded && !embedded && !options.headless {
        tracing::warn!("this libmpv has no render API; using mpv's own window");
    }
    embedded
}

/// Runs the worker that turns mpv events into status updates on its own thread.
#[tracing::instrument(level = "debug", skip_all, err)]
fn start_event_thread(worker: EventWorker) -> Result<JoinHandle<()>, PlayerError> {
    std::thread::Builder::new()
        .name("mpv-events".to_owned())
        .spawn(move || worker.run())
        .map_err(|error| PlayerError::Unavailable(format!("cannot start event thread: {error}")))
}

/// Starts frame rendering; on failure falls back to mpv's own window.
#[tracing::instrument(level = "debug", skip_all)]
fn start_embedding(handle: &Arc<MpvHandle>) -> (Option<FrameRenderer>, Option<Arc<FrameShared>>) {
    let signal = Arc::new(FrameSignal::default());
    let started =
        MpvRenderContext::create(Arc::clone(handle), Arc::clone(&signal)).and_then(|context| {
            let shared = Arc::new(FrameShared::new(INITIAL_SURFACE.0, INITIAL_SURFACE.1));
            FrameRenderer::start(context, signal, Arc::clone(&shared))
                .map(|renderer| (renderer, shared))
        });
    match started {
        Ok((renderer, shared)) => (Some(renderer), Some(shared)),
        Err(error) => {
            tracing::warn!(%error, "embedded rendering failed; using mpv's own window");
            if let Err(error) = handle.set_property("vo", "gpu") {
                tracing::warn!(%error, "could not switch to a window");
            }
            (None, None)
        }
    }
}

impl VideoPlayer for MpvPlayer {
    fn set_verbose_log(&self, enabled: bool) {
        self.handle.set_log_level(log_level(enabled));
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        self.handle
            .set_property("http-header-fields", &header_fields(&request.headers))?;
        self.handle.set_property("referrer", &request.referer)?;
        self.handle
            .set_property("force-media-title", &request.title)?;
        self.handle.set_property("start", &start_option(request))?;
        // Any subtitle download still running belongs to the previous stream. The generation and
        // the queued files change together under one lock, so the event thread never pairs the new
        // files with the old stream (or the old files with the new one).
        queue_for_new_stream(
            &self.pending_subtitles,
            &self.stream_generation,
            request.subtitles.clone(),
        );
        self.unpause_before_new_stream();
        self.tracker.begin_loading();
        tracing::info!(
            url = %request.url,
            subtitle_files = request.subtitles.len(),
            subtitle_languages = %describe_languages(&request.subtitles),
            "mpv loadfile"
        );
        let outcome = self.handle.command(&["loadfile", &request.url, "replace"]);
        if let Err(error) = &outcome {
            self.tracker.fail(error.to_string());
        }
        outcome
    }

    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        tracing::info!(
            files = subtitles.len(),
            languages = %describe_languages(subtitles),
            "subtitle files found after the stream started"
        );
        // The event thread attaches whatever is waiting once the stream has loaded.
        self.pending_subtitles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(subtitles);
    }

    #[tracing::instrument(level = "debug", skip_all, fields(paused = ?paused), err)]
    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        self.handle
            .set_property("pause", if paused { "yes" } else { "no" })
    }

    #[tracing::instrument(level = "debug", skip_all, fields(position_ms = ?position_ms), err)]
    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        let seconds = format!("{:.3}", position_ms.max(0) as f64 / 1000.0);
        self.handle.command(&["seek", &seconds, "absolute"])
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    fn stop(&self) -> Result<(), PlayerError> {
        self.stream_generation.fetch_add(1, Ordering::SeqCst);
        let outcome = self.handle.command(&["stop"]);
        // mpv keeps `pause` across files, so a stream stopped while paused would make the next one
        // start paused; the status and picture are reset here instead of waiting for mpv's event.
        self.unpause_before_new_stream();
        self.tracker.reset_to_idle();
        if let Some(frames) = &self.frames {
            frames.clear();
        }
        outcome
    }

    #[tracing::instrument(level = "debug", skip_all, fields(percent = ?percent), err)]
    fn set_volume(&self, percent: u8) -> Result<(), PlayerError> {
        self.handle
            .set_property("volume", &percent.min(MAX_VOLUME_PERCENT).to_string())
    }

    fn status(&self) -> PlaybackStatus {
        self.tracker.snapshot()
    }

    fn tracks(&self) -> Vec<MediaTrack> {
        self.tracker.tracks()
    }

    fn chapters(&self) -> Vec<player_api::Chapter> {
        self.tracker.chapters()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(kind = ?kind, id = ?id), err)]
    fn select_track(&self, kind: TrackKind, id: Option<i64>) -> Result<(), PlayerError> {
        let property = match kind {
            TrackKind::Audio => "aid",
            TrackKind::Subtitle => "sid",
        };
        let value = id.map_or_else(|| "no".to_owned(), |id| id.to_string());
        self.handle.set_property(property, &value)?;
        if kind == TrackKind::Subtitle {
            // From now on the viewer decides what shows.
            let generation = self.stream_generation.load(Ordering::SeqCst);
            self.subtitle_preference.mark_changed_by_viewer(generation);
        }
        // Re-read at once so the menu shows the new choice without waiting for the next refresh.
        self.tracker
            .set_tracks(read_tracks(|name| self.handle.get_string(name)));
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(language = ?language.map(|l| l.name)))]
    fn set_preferred_audio(&self, language: Option<Language>) {
        self.audio_preference.set(language);
        // mpv's own preference covers streams whose tracks carry language tags; the event thread
        // covers the rest (for example tracks that only have a title).
        if let Err(error) = self
            .handle
            .set_property("alang", &alang_value(language.as_ref()))
        {
            tracing::warn!(%error, "could not tell mpv the preferred audio language");
        }
    }

    fn set_preferred_subtitles(&self, languages: &[Language]) {
        self.subtitle_preference.set_languages(languages.to_vec());
    }

    fn is_embedded(&self) -> bool {
        self.frames.is_some()
    }

    fn set_surface_size(&self, width: u32, height: u32) {
        if let Some(frames) = &self.frames {
            frames.set_size(width, height);
        }
    }

    fn latest_frame(&self, last_seen_serial: u64) -> Option<Arc<VideoFrame>> {
        self.frames.as_ref()?.latest(last_seen_serial)
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.event_thread.take() {
            if thread.join().is_err() {
                tracing::warn!("mpv event thread panicked");
            }
        }
        // `renderer` (declared before `handle`) is dropped next and joins its thread.
    }
}
