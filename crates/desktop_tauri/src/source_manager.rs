//! Finds and tests the streams of the video that is being opened and tells the player what it
//! found.
//!
//! Runs only when a video is opened. The provider's streams are checked as they arrive; playback
//! starts on the best working one a moment after the first is known (or when the viewer skips),
//! and the search and checks go on in the background until they are done or the video is closed.
//! The picker lists what the checks found.

use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use app_services::{CheckQueue, LinkSearch, PlaybackItem, StreamChecker};
use tauri::{AppHandle, Emitter};

use crate::check_gate::{CheckGate, CheckProgress, WaitLimits};
use crate::failure_tally::FailureTally;
use crate::source_discovery::{Discovery, SubtitleSink};
use crate::source_session::{ActiveSources, SourceSession};
use crate::subtitle_delivery::{Delivery, DeliveryParts, SubtitleWork};
use subtitle_store::{KeepGoing, SubtitleLoader};

/// Event the UI listens to so it can reload the list of streams.
pub const SOURCES_CHANGED_EVENT: &str = "sources-changed";
/// Event carrying [`CheckProgress`] while the streams are being found and checked.
pub const SOURCE_CHECK_PROGRESS_EVENT: &str = "source-check-progress";

/// Something the UI should hear about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckUpdate {
    /// The counts changed.
    Progress(CheckProgress),
    /// The list of streams worth offering changed.
    SourcesChanged,
}

/// How the UI hears about the search and the checks.
pub type Tell = Arc<dyn Fn(CheckUpdate) + Send + Sync>;

/// What [`SourceManager::discover_with`] needs to follow one video's search.
pub struct SearchRequest {
    /// The wait of the video that is opening.
    pub gate: Arc<CheckGate>,
    /// Generation of the session opened for it.
    pub generation: u64,
    /// The provider's running search.
    pub search: LinkSearch,
    /// Where subtitles found after playback started go.
    pub give_subtitles: SubtitleSink,
    /// What the title's subtitles need: its key in the saved lists and the language order.
    pub subtitles: SubtitleWork,
}

/// Tauri-managed owner of the current streams and the checks that test them.
pub struct SourceManager {
    session: Arc<SourceSession>,
    checker: StreamChecker,
    /// Downloads and keeps the subtitle files of the provider.
    subtitles: Arc<SubtitleLoader>,
    /// How long a video waits for its provider.
    limits: WaitLimits,
    /// The wait of the video that is being opened or played.
    current: Mutex<Option<Arc<CheckGate>>>,
}

impl SourceManager {
    /// Creates the manager.
    pub fn new(checker: StreamChecker, subtitles: Arc<SubtitleLoader>) -> Self {
        Self {
            session: Arc::new(SourceSession::default()),
            checker,
            subtitles,
            limits: WaitLimits::default(),
            current: Mutex::new(None),
        }
    }

    /// The same manager with other waiting times, so tests do not have to wait for real ones.
    #[cfg(test)]
    pub fn with_limits(mut self, limits: WaitLimits) -> Self {
        self.limits = limits;
        self
    }

    /// The streams of the item that is playing.
    pub fn session(&self) -> &SourceSession {
        &self.session
    }

    /// Starts the opening of a new video and cancels the one before it, if any.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn begin(&self) -> Arc<CheckGate> {
        let gate = Arc::new(CheckGate::new(self.limits));
        let previous = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(Arc::clone(&gate));
        if let Some(previous) = previous {
            previous.cancel();
        }
        gate
    }

    /// Opens an empty list of streams for `item` and returns its generation number, or `None` when
    /// `gate` is no longer the video being opened (the viewer left or opened another one meanwhile),
    /// in which case the list of the newer video is left alone. The check and the change happen
    /// under one lock, so they cannot interleave with [`SourceManager::begin`] or `close`.
    pub fn open(&self, gate: &Arc<CheckGate>, item: PlaybackItem) -> Option<u64> {
        let current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        if !current.as_ref().is_some_and(|open| Arc::ptr_eq(open, gate)) {
            return None;
        }
        Some(
            self.session
                .set(ActiveSources::new(item, Vec::new(), Vec::new())),
        )
    }

    /// The video was closed: stops waiting, stops checking and forgets the streams.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn close(&self) {
        let gate = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(gate) = gate {
            gate.cancel();
        }
        self.session.clear();
    }

    /// The viewer asked to start without waiting for the rest of the checks.
    pub fn skip(&self, app: &AppHandle) {
        self.skip_with(&|update| emit(app, update));
    }

    /// [`SourceManager::skip`], telling `tell` instead of the app window.
    #[tracing::instrument(level = "debug", skip_all)]
    pub(crate) fn skip_with(&self, tell: &dyn Fn(CheckUpdate)) {
        let gate = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match gate {
            Some(gate) => tell(CheckUpdate::Progress(gate.skip())),
            None => tracing::debug!("skip asked while no video is opening"),
        }
    }

    /// Follows the provider's search, downloads its subtitle files and checks its streams in the
    /// background; `tell` hears about progress. Returns the threads that do the work, which end
    /// when everything is done or the video is closed.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn discover_with(&self, tell: Tell, request: SearchRequest) -> Vec<JoinHandle<()>> {
        let SearchRequest {
            gate,
            generation,
            search,
            give_subtitles,
            subtitles,
        } = request;
        tell(CheckUpdate::Progress(gate.progress()));
        let delivery = Delivery::new(DeliveryParts {
            session: Arc::clone(&self.session),
            generation,
            loader: Arc::clone(&self.subtitles),
            work: subtitles,
            tell: Arc::clone(&tell),
            give_subtitles,
        });
        delivery.offer_saved();
        let (prefetch, mut threads) = delivery.start_downloads(self.keep_going(&gate, generation));
        let job = Arc::new(Discovery {
            checker: self.checker.clone(),
            session: Arc::clone(&self.session),
            gate: Arc::clone(&gate),
            generation,
            queue: Arc::new(CheckQueue::default()),
            started: Instant::now(),
            tell,
            prefetch,
            hosts: Mutex::default(),
            failures: FailureTally::default(),
        });
        threads.extend(start_search_and_checks(&job, search));
        threads
    }

    /// Whether anybody still wants the work of the video with this generation.
    fn keep_going(&self, gate: &Arc<CheckGate>, generation: u64) -> KeepGoing {
        let (session, gate) = (Arc::clone(&self.session), Arc::clone(gate));
        Arc::new(move || session.is_current(generation) && !gate.is_cancelled())
    }
}

/// Starts the search thread and the checks thread; if either cannot start, the work is given up.
fn start_search_and_checks(job: &Arc<Discovery>, search: LinkSearch) -> Vec<JoinHandle<()>> {
    let searching = Arc::clone(job);
    let searched = spawn("source-search", move || searching.run_search(search));
    let checking = Arc::clone(job);
    let checks = spawn("source-checks", move || checking.run_checks());
    if searched.is_none() || checks.is_none() {
        job.queue.stop();
        job.prefetch.stop();
        job.gate
            .discovery_ended(Some("Cannot start looking for streams.".to_owned()));
    }
    searched.into_iter().chain(checks).collect()
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> Option<JoinHandle<()>> {
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(work);
    if let Err(error) = &spawned {
        tracing::error!(%error, name, "cannot start a thread for the streams");
    }
    spawned.ok()
}

/// Tells the UI about the search and the checks.
pub(crate) fn emit(app: &AppHandle, update: CheckUpdate) {
    let sent = match update {
        CheckUpdate::Progress(progress) => app.emit(SOURCE_CHECK_PROGRESS_EVENT, progress),
        CheckUpdate::SourcesChanged => app.emit(SOURCES_CHANGED_EVENT, ()),
    };
    if let Err(error) = sent {
        tracing::warn!(%error, "could not tell the UI about the streams");
    }
}

#[cfg(test)]
#[path = "source_manager_tests.rs"]
mod tests;
