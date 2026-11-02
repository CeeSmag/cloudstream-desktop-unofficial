use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use player_api::SubtitleSource;

use crate::mpv_api::MpvHandle;
use crate::mpv_event::MpvEvent;
use crate::mpv_option_format::subtitle_label;
use crate::status_tracker::StatusTracker;
use crate::track_reader::read_tracks;

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
}

impl EventWorker {
    pub(crate) fn run(self) {
        let mut ticks: u32 = 0;
        while !self.stop_flag.load(Ordering::Relaxed) {
            let event = self.handle.wait_event(EVENT_WAIT);
            self.tracker.apply(event);
            if event == MpvEvent::FileLoaded {
                self.attach_subtitles();
                self.refresh_tracks();
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

    /// Re-reads the track list (subtitles added after loading show up here too).
    pub(crate) fn refresh_tracks(&self) {
        self.tracker
            .set_tracks(read_tracks(|name| self.handle.get_string(name)));
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
