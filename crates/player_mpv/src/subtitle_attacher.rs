//! Loads a stream's external subtitle files in the background.
//!
//! A provider can offer dozens of subtitle files and each download may take many seconds or
//! fail, so they must never run on the thread that reports playback progress. Every file's
//! outcome is logged (which language, how long, what mpv said), and so is a summary per stream,
//! so a missing language can be traced to the file that failed.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use player_api::{describe_languages, PlayerError, SubtitleSource};

use crate::mpv_api::MpvHandle;
use crate::mpv_option_format::subtitle_label;

/// How many subtitle files are downloaded at the same time.
const WORKERS: usize = 4;

/// Which stream is current; bumped every time a new stream starts so late workers stop.
pub(crate) type StreamGeneration = Arc<AtomicU64>;

/// How the files of each language fared: (attached, failed).
type Outcomes = BTreeMap<String, (usize, usize)>;

/// What all workers of one batch share, so the last one to finish can log the summary.
struct Batch {
    generation: u64,
    files: usize,
    started: Instant,
    workers_left: AtomicUsize,
    outcomes: Mutex<Outcomes>,
}

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
    tracing::info!(
        generation,
        files = subtitles.len(),
        languages = %describe_languages(&subtitles),
        workers = worker_count,
        "attaching subtitle files"
    );
    let batch = Arc::new(Batch {
        generation,
        files: subtitles.len(),
        started: Instant::now(),
        workers_left: AtomicUsize::new(worker_count),
        outcomes: Mutex::new(Outcomes::new()),
    });
    let queue = Arc::new(Mutex::new(VecDeque::from(subtitles)));
    for index in 0..worker_count {
        let job = Job {
            handle: Arc::clone(handle),
            current: Arc::clone(current),
            batch: Arc::clone(&batch),
            queue: Arc::clone(&queue),
            worker: index,
        };
        let spawned = std::thread::Builder::new()
            .name(format!("mpv-subtitles-{index}"))
            .spawn(move || job.run());
        if let Err(error) = spawned {
            batch.workers_left.fetch_sub(1, Ordering::SeqCst);
            tracing::warn!(%error, "cannot start a subtitle worker");
        }
    }
}

struct Job {
    handle: Arc<MpvHandle>,
    current: StreamGeneration,
    batch: Arc<Batch>,
    queue: Arc<Mutex<VecDeque<SubtitleSource>>>,
    worker: usize,
}

impl Job {
    #[tracing::instrument(level = "debug", skip_all)]
    fn run(self) {
        while self.current.load(Ordering::SeqCst) == self.batch.generation {
            let Some(subtitle) = self.next() else { break };
            self.attach(&subtitle);
        }
        self.finish();
    }

    fn attach(&self, subtitle: &SubtitleSource) {
        let label = subtitle_label(subtitle);
        let started = Instant::now();
        let result = self
            .handle
            .command(&["sub-add", &subtitle.url, "auto", label]);
        self.log_outcome(subtitle, &result, started.elapsed().as_millis());
        let mut outcomes = self
            .batch
            .outcomes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let entry = outcomes.entry(label.to_owned()).or_default();
        if result.is_ok() {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }

    fn log_outcome(
        &self,
        subtitle: &SubtitleSource,
        result: &Result<(), PlayerError>,
        elapsed_ms: u128,
    ) {
        let label = subtitle_label(subtitle);
        let still_current = self.current.load(Ordering::SeqCst) == self.batch.generation;
        match result {
            Ok(()) if !still_current => tracing::warn!(
                language = label,
                url = %subtitle.url,
                elapsed_ms,
                "subtitle attached after its stream was replaced; it may have landed on the newer stream"
            ),
            Ok(()) => tracing::debug!(
                language = label,
                url = %subtitle.url,
                worker = self.worker,
                elapsed_ms,
                "subtitle attached"
            ),
            Err(error) => tracing::warn!(
                %error,
                language = label,
                url = %subtitle.url,
                worker = self.worker,
                elapsed_ms,
                since_batch_start_ms = self.batch.started.elapsed().as_millis(),
                still_current,
                "could not attach subtitle"
            ),
        }
    }

    /// The last worker to finish logs how the whole batch went.
    fn finish(&self) {
        if self.batch.workers_left.fetch_sub(1, Ordering::SeqCst) != 1 {
            return;
        }
        let not_tried = self
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len();
        let outcomes = self
            .batch
            .outcomes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (attached, failed) = outcomes
            .values()
            .fold((0, 0), |(a, f), (ok, bad)| (a + ok, f + bad));
        let by_language = outcomes
            .iter()
            .map(|(language, (ok, bad))| format!("{language}: {ok} attached, {bad} failed"))
            .collect::<Vec<_>>()
            .join("; ");
        tracing::info!(
            generation = self.batch.generation,
            files = self.batch.files,
            attached,
            failed,
            not_tried,
            replaced_meanwhile = self.current.load(Ordering::SeqCst) != self.batch.generation,
            elapsed_ms = self.batch.started.elapsed().as_millis(),
            %by_language,
            "subtitle files finished"
        );
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn next(&self) -> Option<SubtitleSource> {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }
}
