//! Loads a stream's external subtitle files in the background.
//!
//! A provider can offer dozens of subtitle files and each download may take many seconds or
//! fail, so they must never run on the thread that reports playback progress.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use player_api::SubtitleSource;

use crate::mpv_api::MpvHandle;
use crate::mpv_option_format::subtitle_label;

/// How many subtitle files are downloaded at the same time.
const WORKERS: usize = 4;

/// Which stream is current; bumped every time a new stream starts so late workers stop.
pub(crate) type StreamGeneration = Arc<AtomicU64>;

/// Starts workers that add `subtitles` to the stream identified by `generation`. They stop as
/// soon as another stream has started, so a slow download can never land on the wrong stream.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn attach_in_background(
    handle: &Arc<MpvHandle>,
    current: &StreamGeneration,
    generation: u64,
    subtitles: Vec<SubtitleSource>,
) {
    let worker_count = WORKERS.min(subtitles.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(subtitles)));
    for index in 0..worker_count {
        let job = Job {
            handle: Arc::clone(handle),
            current: Arc::clone(current),
            generation,
            queue: Arc::clone(&queue),
        };
        let spawned = std::thread::Builder::new()
            .name(format!("mpv-subtitles-{index}"))
            .spawn(move || job.run());
        if let Err(error) = spawned {
            tracing::warn!(%error, "cannot start a subtitle worker");
        }
    }
}

struct Job {
    handle: Arc<MpvHandle>,
    current: StreamGeneration,
    generation: u64,
    queue: Arc<Mutex<VecDeque<SubtitleSource>>>,
}

impl Job {
    #[tracing::instrument(level = "debug", skip_all)]
    fn run(self) {
        while self.current.load(Ordering::SeqCst) == self.generation {
            let Some(subtitle) = self.next() else { break };
            let label = subtitle_label(&subtitle);
            if let Err(error) = self
                .handle
                .command(&["sub-add", &subtitle.url, "auto", label])
            {
                tracing::warn!(%error, url = %subtitle.url, "could not attach subtitle");
            }
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn next(&self) -> Option<SubtitleSource> {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }
}
