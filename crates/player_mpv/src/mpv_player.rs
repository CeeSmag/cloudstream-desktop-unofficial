use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use player_api::{
    PlaybackRequest, PlaybackStatus, PlayerError, SubtitleSource, VideoFrame, VideoPlayer,
};

use crate::event_worker::EventWorker;
use crate::frame_renderer::{FrameRenderer, FrameShared};
use crate::mpv_api::{MpvApi, MpvHandle};
use crate::mpv_option_format::{header_fields, start_option};
use crate::mpv_render_api::{FrameSignal, MpvRenderContext};
use crate::player_configuration::configure;
pub use crate::player_configuration::PlayerOptions;
use crate::status_tracker::StatusTracker;

/// Size used for the first frames, before the UI reports its real picture area.
const INITIAL_SURFACE: (u32, u32) = (1280, 720);

/// `VideoPlayer` backed by libmpv: embedded (pictures handed to the app) or in mpv's own window.
pub struct MpvPlayer {
    tracker: Arc<StatusTracker>,
    pending_subtitles: Arc<Mutex<Vec<SubtitleSource>>>,
    stop_flag: Arc<AtomicBool>,
    event_thread: Option<JoinHandle<()>>,
    frames: Option<Arc<FrameShared>>,
    // Dropped before `handle` (declaration order) so the render context never outlives mpv.
    _renderer: Option<FrameRenderer>,
    handle: Arc<MpvHandle>,
}

impl MpvPlayer {
    /// Loads libmpv from `library_path` and starts an idle player.
    pub fn new(library_path: &Path, options: &PlayerOptions) -> Result<Self, PlayerError> {
        let api = Arc::new(MpvApi::load(library_path)?);
        let embedded = options.embedded && !options.headless && api.render_api().is_some();
        if options.embedded && !embedded && !options.headless {
            tracing::warn!("this libmpv has no render API; using mpv's own window");
        }
        let handle = Arc::new(MpvHandle::create(api)?);
        configure(&handle, options, embedded)?;
        handle.initialize()?;
        let (renderer, frames) = if embedded {
            start_embedding(&handle)
        } else {
            (None, None)
        };
        let tracker = Arc::new(StatusTracker::default());
        let pending_subtitles = Arc::new(Mutex::new(Vec::new()));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let worker = EventWorker {
            handle: Arc::clone(&handle),
            tracker: Arc::clone(&tracker),
            pending_subtitles: Arc::clone(&pending_subtitles),
            stop_flag: Arc::clone(&stop_flag),
        };
        let event_thread = std::thread::Builder::new()
            .name("mpv-events".to_owned())
            .spawn(move || worker.run())
            .map_err(|error| {
                PlayerError::Unavailable(format!("cannot start event thread: {error}"))
            })?;
        Ok(Self {
            tracker,
            pending_subtitles,
            stop_flag,
            event_thread: Some(event_thread),
            frames,
            _renderer: renderer,
            handle,
        })
    }
}

/// Starts frame rendering; on failure falls back to mpv's own window.
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
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        self.handle
            .set_property("http-header-fields", &header_fields(&request.headers))?;
        self.handle.set_property("referrer", &request.referer)?;
        self.handle
            .set_property("force-media-title", &request.title)?;
        self.handle.set_property("start", &start_option(request))?;
        *self
            .pending_subtitles
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = request.subtitles.clone();
        self.tracker.begin_loading();
        tracing::info!(url = %request.url, "mpv loadfile");
        let outcome = self.handle.command(&["loadfile", &request.url, "replace"]);
        if let Err(error) = &outcome {
            self.tracker.fail(error.to_string());
        }
        outcome
    }

    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        self.handle
            .set_property("pause", if paused { "yes" } else { "no" })
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        let seconds = format!("{:.3}", position_ms.max(0) as f64 / 1000.0);
        self.handle.command(&["seek", &seconds, "absolute"])
    }

    fn stop(&self) -> Result<(), PlayerError> {
        self.handle.command(&["stop"])
    }

    fn status(&self) -> PlaybackStatus {
        self.tracker.snapshot()
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
