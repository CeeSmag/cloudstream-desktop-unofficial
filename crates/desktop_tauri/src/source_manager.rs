//! Finds and tests the streams of the video that is being opened and tells the player what it
//! found.
//!
//! Runs only when a video is opened. The provider's streams are checked as they arrive; playback
//! starts on the best working one a moment after the first is known (or when the viewer skips),
//! and the search and checks go on in the background until they are done or the video is closed.
//! The picker lists what the checks found.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use app_services::{CheckQueue, LinkSearch, PlaybackItem, StreamChecker};
use tauri::{AppHandle, Emitter};

use crate::check_gate::{CheckGate, CheckProgress, Grace};
use crate::source_discovery::{Discovery, SubtitleSink};
use crate::source_session::{ActiveSources, SourceSession};

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

/// What [`SourceManager::discover`] needs to follow one video's search.
pub struct SearchRequest {
    /// The wait of the video that is opening.
    pub gate: Arc<CheckGate>,
    /// Generation of the session opened for it.
    pub generation: u64,
    /// The provider's running search.
    pub search: LinkSearch,
    /// Where subtitles found after playback started go.
    pub give_subtitles: SubtitleSink,
}

/// Tauri-managed owner of the current streams and the checks that test them.
pub struct SourceManager {
    session: Arc<SourceSession>,
    checker: StreamChecker,
    /// The wait of the video that is being opened or played.
    current: Mutex<Option<Arc<CheckGate>>>,
}

impl SourceManager {
    /// Creates the manager.
    pub fn new(checker: StreamChecker) -> Self {
        Self {
            session: Arc::new(SourceSession::default()),
            checker,
            current: Mutex::new(None),
        }
    }

    /// The streams of the item that is playing.
    pub fn session(&self) -> &SourceSession {
        &self.session
    }

    /// Starts the opening of a new video and cancels the one before it, if any.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn begin(&self) -> Arc<CheckGate> {
        let gate = Arc::new(CheckGate::new(Grace::default()));
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

    /// Opens an empty list of streams for `item`; returns its generation number.
    pub fn open(&self, item: PlaybackItem) -> u64 {
        self.session
            .set(ActiveSources::new(item, Vec::new(), Vec::new()))
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
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn skip(&self, app: &AppHandle) {
        let gate = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match gate {
            Some(gate) => emit(app, CheckUpdate::Progress(gate.skip())),
            None => tracing::debug!("skip asked while no video is opening"),
        }
    }

    /// Follows the provider's search and checks its streams in the background.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn discover(&self, app: AppHandle, request: SearchRequest) {
        let SearchRequest {
            gate,
            generation,
            search,
            give_subtitles,
        } = request;
        emit(&app, CheckUpdate::Progress(gate.progress()));
        let job = Arc::new(Discovery {
            checker: self.checker.clone(),
            session: Arc::clone(&self.session),
            gate: Arc::clone(&gate),
            generation,
            queue: Arc::new(CheckQueue::default()),
            started: Instant::now(),
            tell: Arc::new(move |update| emit(&app, update)),
            give_subtitles,
        });
        let searching = Arc::clone(&job);
        let searched = spawn("source-search", move || searching.run_search(search));
        let checking = Arc::clone(&job);
        let checks = spawn("source-checks", move || checking.run_checks());
        if !(searched && checks) {
            job.queue.stop();
            gate.discovery_ended(Some("Cannot start looking for streams.".to_owned()));
        }
    }
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> bool {
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(work);
    if let Err(error) = &spawned {
        tracing::error!(%error, name, "cannot start a thread for the streams");
    }
    spawned.is_ok()
}

fn emit(app: &AppHandle, update: CheckUpdate) {
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
