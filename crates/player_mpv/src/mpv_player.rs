use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use player_api::{PlaybackRequest, PlaybackStatus, PlayerError, SubtitleSource, VideoPlayer};

use crate::mpv_api::{MpvApi, MpvHandle};
use crate::mpv_event::MpvEvent;
use crate::mpv_option_format::{header_fields, start_option, subtitle_label};
use crate::status_tracker::StatusTracker;

/// How long the event thread waits per iteration; also its stop-latency.
const EVENT_WAIT: Duration = Duration::from_millis(200);

/// How the player should be configured.
#[derive(Debug, Clone, Default)]
pub struct PlayerOptions {
    /// Run without video/audio output (tests and CI).
    pub headless: bool,
}

/// `VideoPlayer` backed by libmpv, with its own window and on-screen controller.
pub struct MpvPlayer {
    handle: Arc<MpvHandle>,
    tracker: Arc<StatusTracker>,
    pending_subtitles: Arc<Mutex<Vec<SubtitleSource>>>,
    stop_flag: Arc<AtomicBool>,
    event_thread: Option<JoinHandle<()>>,
}

impl MpvPlayer {
    /// Loads libmpv from `library_path` and starts an idle player.
    pub fn new(library_path: &Path, options: &PlayerOptions) -> Result<Self, PlayerError> {
        let api = Arc::new(MpvApi::load(library_path)?);
        let handle = Arc::new(MpvHandle::create(api)?);
        configure(&handle, options)?;
        handle.initialize()?;
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
            handle,
            tracker,
            pending_subtitles,
            stop_flag,
            event_thread: Some(event_thread),
        })
    }
}

fn configure(handle: &MpvHandle, options: &PlayerOptions) -> Result<(), PlayerError> {
    let fixed = [
        ("idle", "yes"),
        ("keep-open", "no"),
        ("osc", "yes"),
        ("input-default-bindings", "yes"),
        ("input-vo-keyboard", "yes"),
        ("hwdec", "auto-safe"),
        ("ytdl", "no"),
    ];
    for (name, value) in fixed {
        handle.set_option(name, value)?;
    }
    if std::env::var_os("CS_MPV_LOG").is_some() {
        // Developer aid: mpv's own log to the console for diagnosing stream failures.
        handle.set_option("terminal", "yes")?;
        handle.set_option("msg-level", "all=v")?;
    }
    if options.headless {
        for (name, value) in [("vo", "null"), ("ao", "null"), ("force-window", "no")] {
            handle.set_option(name, value)?;
        }
    } else {
        handle.set_option("force-window", "yes")?;
    }
    Ok(())
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
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.event_thread.take() {
            if thread.join().is_err() {
                tracing::warn!("mpv event thread panicked");
            }
        }
    }
}

/// Background loop that turns mpv events and property polls into status updates.
struct EventWorker {
    handle: Arc<MpvHandle>,
    tracker: Arc<StatusTracker>,
    pending_subtitles: Arc<Mutex<Vec<SubtitleSource>>>,
    stop_flag: Arc<AtomicBool>,
}

impl EventWorker {
    fn run(self) {
        while !self.stop_flag.load(Ordering::Relaxed) {
            let event = self.handle.wait_event(EVENT_WAIT);
            self.tracker.apply(event);
            if event == MpvEvent::FileLoaded {
                self.attach_subtitles();
            }
            if event == MpvEvent::Shutdown {
                break;
            }
            self.poll_progress();
        }
    }

    fn attach_subtitles(&self) {
        let subtitles = std::mem::take(
            &mut *self
                .pending_subtitles
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for subtitle in &subtitles {
            let label = subtitle_label(subtitle);
            if let Err(error) = self
                .handle
                .command(&["sub-add", &subtitle.url, "auto", label])
            {
                tracing::warn!(%error, url = %subtitle.url, "could not attach subtitle");
            }
        }
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
