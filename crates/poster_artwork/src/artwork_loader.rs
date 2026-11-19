//! Loads posters on a few worker threads: one download per address however many tiles ask.
//!
//! The newest request is served first (the screen the viewer just opened), every request gets
//! exactly one answer (also when the loader stops), and an address loaded once is answered from
//! memory, then from disk, before the network.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use crate::artwork_load;
use crate::artwork_memory_cache::MemoryCache;
use crate::{
    poster_address, ArtworkError, ArtworkFetcher, ArtworkHeaders, DecodedArtwork, DiskCache,
};

/// What a request gets back.
pub type ArtworkResult = Result<Arc<DecodedArtwork>, ArtworkError>;
/// Called once with the outcome of a request, on a worker thread (or the caller's, when cached).
pub type ArtworkCallback = Box<dyn FnOnce(ArtworkResult) + Send>;

/// How many posters download at once by default: enough to fill a screen quickly, few enough
/// not to hammer one image host.
pub const DEFAULT_WORKERS: usize = 6;
/// Decoded posters kept in memory by default (about 190 tiles of 360x540).
pub const DEFAULT_MEMORY_BUDGET_BYTES: usize = 144 * 1024 * 1024;

/// How the loader is set up.
pub struct ArtworkLoaderConfig {
    /// Worker threads (at least one).
    pub workers: usize,
    /// Bytes of decoded posters kept in memory.
    pub memory_budget_bytes: usize,
    /// Folder for downloaded files, if any.
    pub disk: Option<DiskCache>,
}

impl Default for ArtworkLoaderConfig {
    fn default() -> Self {
        Self {
            workers: DEFAULT_WORKERS,
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
            disk: None,
        }
    }
}

pub(crate) struct Job {
    pub(crate) url: String,
    pub(crate) headers: ArtworkHeaders,
    /// When false, skip disk load/save (details poster); memory cache still used.
    pub(crate) persist: bool,
}

struct JobSpec<'a> {
    raw_url: &'a str,
    headers: ArtworkHeaders,
    persist: bool,
    on_done: ArtworkCallback,
}

struct QueueState {
    /// Waiting downloads, newest last (served first).
    jobs: Vec<Job>,
    /// Everyone waiting for an address that is queued or loading.
    waiting: HashMap<String, Vec<ArtworkCallback>>,
    closed: bool,
}

pub(crate) struct Shared {
    state: Mutex<QueueState>,
    work_ready: Condvar,
    memory: Mutex<MemoryCache>,
    pub(crate) disk: Option<DiskCache>,
    pub(crate) fetcher: Arc<dyn ArtworkFetcher>,
    live_workers: AtomicUsize,
}

impl Shared {
    fn lock_state(&self) -> MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_memory(&self) -> MutexGuard<'_, MemoryCache> {
        self.memory.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Downloads, shrinks and caches posters. Dropping it stops the workers.
pub struct ArtworkLoader {
    shared: Arc<Shared>,
}

impl ArtworkLoader {
    /// Starts the worker threads.
    pub fn new(fetcher: Arc<dyn ArtworkFetcher>, config: ArtworkLoaderConfig) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(QueueState {
                jobs: Vec::new(),
                waiting: HashMap::new(),
                closed: false,
            }),
            work_ready: Condvar::new(),
            memory: Mutex::new(MemoryCache::new(config.memory_budget_bytes)),
            disk: config.disk,
            fetcher,
            live_workers: AtomicUsize::new(0),
        });
        for index in 0..config.workers.max(1) {
            start_worker(&shared, index);
        }
        Self { shared }
    }

    /// The poster for `raw_url` when it is already in memory.
    pub fn cached(&self, raw_url: &str) -> Option<Arc<DecodedArtwork>> {
        let url = poster_address(raw_url).ok()?;
        self.shared.lock_memory().get(&url)
    }

    /// Loads `raw_url` (sending `headers` if it has to be downloaded) and calls `on_done` once.
    pub fn request(&self, raw_url: &str, headers: ArtworkHeaders, on_done: ArtworkCallback) {
        self.enqueue(JobSpec {
            raw_url,
            headers,
            persist: true,
            on_done,
        });
    }

    /// Like [`Self::request`], but never reads or writes the disk cache (details poster).
    pub fn request_ephemeral(
        &self,
        raw_url: &str,
        headers: ArtworkHeaders,
        on_done: ArtworkCallback,
    ) {
        self.enqueue(JobSpec {
            raw_url,
            headers,
            persist: false,
            on_done,
        });
    }

    /// Drops a decoded poster from memory (details Back). Disk files are untouched.
    pub fn forget_memory(&self, raw_url: &str) {
        let Ok(url) = poster_address(raw_url) else {
            return;
        };
        let _ = self.shared.lock_memory().forget(&url);
    }

    fn enqueue(&self, spec: JobSpec<'_>) {
        let url = match poster_address(spec.raw_url) {
            Ok(url) => url,
            Err(error) => return (spec.on_done)(Err(error)),
        };
        let mut state = self.shared.lock_state();
        if state.closed {
            drop(state);
            return (spec.on_done)(Err(ArtworkError::ShutDown));
        }
        // Checked under the queue lock: a worker files its result in memory under the same lock,
        // so an address is never downloaded twice by a request racing its own completion.
        let hit = self.shared.lock_memory().get(&url);
        if let Some(artwork) = hit {
            drop(state);
            return (spec.on_done)(Ok(artwork));
        }
        if let Some(waiters) = state.waiting.get_mut(&url) {
            waiters.push(spec.on_done);
            if spec.persist {
                if let Some(job) = state.jobs.iter_mut().rev().find(|job| job.url == url) {
                    job.persist = true;
                }
            }
            return;
        }
        state.waiting.insert(url.clone(), vec![spec.on_done]);
        state.jobs.push(Job {
            url,
            headers: spec.headers,
            persist: spec.persist,
        });
        drop(state);
        self.shared.work_ready.notify_one();
    }

    /// Stops the workers; every request still waiting is answered with [`ArtworkError::ShutDown`].
    pub fn shutdown(&self) {
        let waiting = {
            let mut state = self.shared.lock_state();
            state.closed = true;
            state.jobs.clear();
            std::mem::take(&mut state.waiting)
        };
        self.shared.work_ready.notify_all();
        for on_done in waiting.into_values().flatten() {
            on_done(Err(ArtworkError::ShutDown));
        }
    }

    /// Worker threads still running (they end shortly after [`Self::shutdown`]).
    pub fn live_workers(&self) -> usize {
        self.shared.live_workers.load(Ordering::SeqCst)
    }
}

impl Drop for ArtworkLoader {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn start_worker(shared: &Arc<Shared>, index: usize) {
    let shared = Arc::clone(shared);
    shared.live_workers.fetch_add(1, Ordering::SeqCst);
    let started = std::thread::Builder::new()
        .name(format!("artwork-{index}"))
        .spawn({
            let shared = Arc::clone(&shared);
            move || {
                run_worker(&shared);
                shared.live_workers.fetch_sub(1, Ordering::SeqCst);
            }
        });
    if let Err(error) = started {
        shared.live_workers.fetch_sub(1, Ordering::SeqCst);
        tracing::error!(%error, "could not start a poster worker");
    }
}

fn run_worker(shared: &Shared) {
    while let Some(job) = next_job(shared) {
        let result = artwork_load::load(shared, &job);
        let waiters = {
            let mut state = shared.lock_state();
            if let Ok(artwork) = &result {
                shared
                    .lock_memory()
                    .insert(job.url.clone(), Arc::clone(artwork));
            }
            state.waiting.remove(&job.url).unwrap_or_default()
        };
        for on_done in waiters {
            on_done(result.clone());
        }
    }
}

fn next_job(shared: &Shared) -> Option<Job> {
    let mut state = shared.lock_state();
    loop {
        if state.closed {
            return None;
        }
        if let Some(job) = state.jobs.pop() {
            return Some(job);
        }
        state = shared
            .work_ready
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner);
    }
}
