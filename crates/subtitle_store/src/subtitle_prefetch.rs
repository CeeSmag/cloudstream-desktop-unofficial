//! Downloads the subtitle files of one video in the background, many at once, the viewer's
//! preferred languages first, and reports each file the moment it is ready.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use core_model::SubtitleTrack;

use crate::host_circuit::{host_of, HostCircuit};
use crate::subtitle_cache::ReadySubtitle;
use crate::subtitle_loader::{LoadError, SubtitleLoader};

/// Called with each subtitle file as soon as it is ready.
pub type ReadyCallback = Arc<dyn Fn(ReadySubtitle) + Send + Sync>;
/// Called when a subtitle track fails to download/normalize or is skipped by the host circuit.
pub type FailureCallback = Arc<dyn Fn(&SubtitleTrack, &str) + Send + Sync>;
/// Asked before every download; `false` means nobody wants the files any more.
pub type KeepGoing = Arc<dyn Fn() -> bool + Send + Sync>;
/// Ranks a language label: lower is more wanted.
pub type LanguageRank = Arc<dyn Fn(&str) -> usize + Send + Sync>;

/// Callbacks invoked by [`SubtitlePrefetch`] workers.
pub struct PrefetchCallbacks {
    /// Called with each subtitle file as soon as it is ready on disk.
    pub on_ready: ReadyCallback,
    /// Called when a subtitle track could not be made ready (not called on intentional stop).
    pub on_failed: FailureCallback,
    /// Asked before every download; `false` stops remaining work.
    pub keep_going: KeepGoing,
}

/// [`LanguageRank`] values at this rank are not downloaded (non-preferred languages).
pub const SKIP_DOWNLOAD_RANK: usize = usize::MAX;

/// How the downloads run.
#[derive(Clone)]
pub struct PrefetchConfig {
    /// How many files are downloaded at the same time. The wait is on the host, not on bandwidth.
    pub workers: usize,
    /// Which languages come first.
    pub rank_of: LanguageRank,
}

/// What the downloads have come to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrefetchSummary {
    /// Files asked for (each address once).
    pub submitted: usize,
    /// Files waiting in the queue.
    pub queued: usize,
    /// Files currently being downloaded or normalized.
    pub in_flight: usize,
    /// Files that are ready.
    pub ready: usize,
    /// Of those, files that were on disk already.
    pub from_cache: usize,
    /// Files that could not be made ready.
    pub failed: usize,
    /// The failure reasons with their counts.
    pub reasons: BTreeMap<String, usize>,
}

struct Job {
    track: SubtitleTrack,
    rank: usize,
    order: usize,
}

#[derive(Default)]
struct State {
    waiting: Vec<Job>,
    seen: HashSet<String>,
    submitted: usize,
    in_flight: usize,
    paused: bool,
    closed: bool,
    stopped: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    summary: Mutex<PrefetchSummary>,
    circuit: HostCircuit,
    loader: Arc<SubtitleLoader>,
    on_ready: ReadyCallback,
    on_failed: FailureCallback,
    keep_going: KeepGoing,
    rank_of: LanguageRank,
    workers_left: AtomicUsize,
    started: Instant,
}

/// The downloads of one video.
#[derive(Clone)]
pub struct SubtitlePrefetch {
    shared: Arc<Shared>,
}

impl SubtitlePrefetch {
    /// Starts the workers; they wait for files to be [`submitted`](Self::submit) and end after
    /// [`finish`](Self::finish) once the queue is empty, or at once on [`stop`](Self::stop).
    pub fn start(
        loader: Arc<SubtitleLoader>,
        config: &PrefetchConfig,
        on_ready: ReadyCallback,
        keep_going: KeepGoing,
    ) -> (Self, Vec<JoinHandle<()>>) {
        let callbacks = PrefetchCallbacks {
            on_ready,
            on_failed: Arc::new(|_, _| {}),
            keep_going,
        };
        Self::start_with_callbacks(loader, config, callbacks)
    }

    /// Starts the workers with both ready and failure callbacks (used to refund budget slots).
    pub fn start_with_callbacks(
        loader: Arc<SubtitleLoader>,
        config: &PrefetchConfig,
        callbacks: PrefetchCallbacks,
    ) -> (Self, Vec<JoinHandle<()>>) {
        let workers = config.workers.max(1);
        let shared = Arc::new(Shared {
            state: Mutex::default(),
            wake: Condvar::new(),
            summary: Mutex::default(),
            circuit: HostCircuit::new(),
            loader,
            on_ready: callbacks.on_ready,
            on_failed: callbacks.on_failed,
            keep_going: callbacks.keep_going,
            rank_of: Arc::clone(&config.rank_of),
            workers_left: AtomicUsize::new(workers),
            started: Instant::now(),
        });
        let handles = (0..workers)
            .filter_map(|number| {
                let worker = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("subtitle-fetch-{number}"))
                    .spawn(move || worker.run())
                    .inspect_err(
                        |error| tracing::error!(%error, "cannot start a subtitle download thread"),
                    )
                    .ok()
            })
            .collect::<Vec<_>>();
        shared.workers_left.store(handles.len(), Ordering::SeqCst);
        (Self { shared }, handles)
    }

    /// Queues files; an address that was queued before is ignored.
    /// Languages ranked [`SKIP_DOWNLOAD_RANK`] are not queued.
    pub fn submit(&self, tracks: Vec<SubtitleTrack>) {
        let mut state = self.shared.lock();
        for track in tracks {
            if track.url.trim().is_empty() || !state.seen.insert(track.url.clone()) {
                continue;
            }
            let rank = (self.shared.rank_of)(&track.lang);
            if rank == SKIP_DOWNLOAD_RANK {
                continue;
            }
            let order = state.submitted;
            state.submitted += 1;
            state.waiting.push(Job { track, rank, order });
        }
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Pauses or resumes pulling new downloads from the queue (in-flight downloads finish cleanly).
    pub fn set_paused(&self, paused: bool) {
        let mut state = self.shared.lock();
        state.paused = paused;
        drop(state);
        self.shared.wake.notify_all();
    }

    /// No more files will be queued; the workers end once the queue is empty (unpauses if paused).
    pub fn finish(&self) {
        let mut state = self.shared.lock();
        state.closed = true;
        state.paused = false;
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Nobody wants the files any more: forgets the queue and ends the workers.
    pub fn stop(&self) {
        let mut state = self.shared.lock();
        state.waiting.clear();
        state.paused = false;
        state.closed = true;
        state.stopped = true;
        drop(state);
        self.shared.wake.notify_all();
    }

    /// What has come of the downloads so far.
    pub fn summary(&self) -> PrefetchSummary {
        let mut summary = self
            .shared
            .summary
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let state = self.shared.lock();
        summary.submitted = state.submitted;
        summary.queued = state.waiting.len();
        summary.in_flight = state.in_flight;
        summary
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The most wanted waiting file (best language, then earliest); waits when empty or paused.
    fn next(&self) -> Option<Job> {
        let mut state = self.lock();
        loop {
            if state.stopped {
                return None;
            }
            if !state.paused {
                let best = state
                    .waiting
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, job)| (job.rank, job.order))
                    .map(|(position, _)| position);
                if let Some(position) = best {
                    state.in_flight += 1;
                    return Some(state.waiting.remove(position));
                }
                if state.closed {
                    return None;
                }
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn run(&self) {
        while let Some(job) = self.next() {
            if !(self.keep_going)() {
                let mut state = self.lock();
                state.waiting.clear();
                state.in_flight = state.in_flight.saturating_sub(1);
                break;
            }
            self.process(&job);
            let mut state = self.lock();
            state.in_flight = state.in_flight.saturating_sub(1);
        }
        if self.workers_left.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.log_summary();
        }
    }

    fn process(&self, job: &Job) {
        let started = Instant::now();
        let outcome = if self.circuit.is_tripped(&job.track.url) {
            let host = host_of(&job.track.url).unwrap_or_default();
            Err(LoadError::HostSkipped { host })
        } else {
            self.loader.load(&job.track, &*self.keep_going)
        };
        let elapsed_ms = started.elapsed().as_millis();
        match outcome {
            Ok(loaded) => self.handle_ready(job, loaded, elapsed_ms),
            Err(LoadError::Stopped) => {}
            Err(error) => self.handle_failure(job, &error, elapsed_ms),
        }
    }

    fn handle_ready(
        &self,
        job: &Job,
        loaded: crate::subtitle_loader::LoadedSubtitle,
        elapsed_ms: u128,
    ) {
        if !loaded.from_cache {
            self.circuit.record_success(&job.track.url);
        }
        let mut summary = self.summary.lock().unwrap_or_else(PoisonError::into_inner);
        summary.ready += 1;
        summary.from_cache += usize::from(loaded.from_cache);
        drop(summary);
        tracing::debug!(
            language = %job.track.lang,
            from_cache = loaded.from_cache,
            elapsed_ms,
            "subtitle file ready"
        );
        (self.on_ready)(loaded.ready);
    }

    fn handle_failure(&self, job: &Job, error: &LoadError, elapsed_ms: u128) {
        if !matches!(error, LoadError::HostSkipped { .. }) {
            if let Some(host) = self.circuit.record_failure(&job.track.url) {
                tracing::warn!(
                    %host,
                    "subtitle host failed repeatedly; skipping remaining files from this host"
                );
            }
        }
        let reason = reason_of(error);
        let mut summary = self.summary.lock().unwrap_or_else(PoisonError::into_inner);
        summary.failed += 1;
        *summary.reasons.entry(reason.clone()).or_default() += 1;
        drop(summary);
        if matches!(error, LoadError::HostSkipped { .. }) {
            tracing::debug!(%error, language = %job.track.lang, url = %job.track.url, "skipped subtitle on failed host");
        } else {
            tracing::warn!(%error, language = %job.track.lang, url = %job.track.url, elapsed_ms, "subtitle file could not be made ready");
        }
        (self.on_failed)(&job.track, &reason);
    }

    fn log_summary(&self) {
        let summary = self
            .summary
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let reasons: Vec<String> = summary
            .reasons
            .iter()
            .map(|(reason, count)| format!("{reason}={count}"))
            .collect();
        tracing::info!(
            submitted = self.lock().submitted,
            ready = summary.ready,
            from_cache = summary.from_cache,
            failed = summary.failed,
            failure_reasons = %reasons.join(", "),
            elapsed_ms = self.started.elapsed().as_millis(),
            "subtitle downloads finished"
        );
    }
}

/// The failure without what is particular to one file, so equal failures count together.
fn reason_of(error: &LoadError) -> String {
    match error {
        LoadError::Download { reason, .. } => format!("download failed ({reason})"),
        LoadError::HostSkipped { .. } => "host skipped".to_owned(),
        other => other.to_string(),
    }
}

#[cfg(test)]
#[path = "subtitle_prefetch_tests.rs"]
mod tests;
