//! Tests the streams of the video that is being opened and tells the player what it found.
//!
//! Runs only when a video is opened: every stream is checked before playback starts, the
//! viewer may skip the wait, and the checks go on in the background until they are done or the
//! video is closed. The picker lists what the checks found.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use app_services::{LinkHealth, StreamChecker};
use core_model::PlayableLink;
use tauri::{AppHandle, Emitter};

use crate::check_gate::{CheckGate, CheckProgress};
use crate::source_session::{ActiveSources, SourceHealth, SourceSession};

/// Event the UI listens to so it can reload the list of streams.
pub const SOURCES_CHANGED_EVENT: &str = "sources-changed";
/// Event carrying [`CheckProgress`] while the streams are being checked.
pub const SOURCE_CHECK_PROGRESS_EVENT: &str = "source-check-progress";

/// The picker is refreshed after this many new results.
const NOTIFY_EVERY: usize = 8;

/// Something the UI should hear about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckUpdate {
    /// The counts changed.
    Progress(CheckProgress),
    /// The list of streams worth offering changed.
    SourcesChanged,
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
        let gate = Arc::new(CheckGate::default());
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

    /// Remembers the streams and checks all of them in the background.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn run_checks(&self, app: AppHandle, gate: &Arc<CheckGate>, sources: ActiveSources) {
        let links: Vec<(usize, PlayableLink)> = sources.links.iter().cloned().enumerate().collect();
        let generation = self.session.set(sources);
        emit(&app, CheckUpdate::Progress(gate.set_total(links.len())));
        let job = BackgroundChecks {
            checker: self.checker.clone(),
            session: Arc::clone(&self.session),
            gate: Arc::clone(gate),
            generation,
            started: Instant::now(),
            tell: Box::new(move |update| emit(&app, update)),
        };
        let spawned = std::thread::Builder::new()
            .name("source-checks".to_owned())
            .spawn(move || job.run(&links));
        if let Err(error) = spawned {
            tracing::warn!(%error, "cannot start the stream checks; playing the first stream");
            gate.finish();
        }
    }
}

fn to_health(found: &LinkHealth) -> SourceHealth {
    if found.is_working() {
        SourceHealth::Working
    } else {
        SourceHealth::Broken
    }
}

/// Everything the check thread needs to test the streams of one title.
struct BackgroundChecks {
    checker: StreamChecker,
    session: Arc<SourceSession>,
    gate: Arc<CheckGate>,
    generation: u64,
    started: Instant,
    tell: Box<dyn Fn(CheckUpdate) + Send + Sync>,
}

impl BackgroundChecks {
    fn wanted(&self) -> bool {
        self.session.is_current(self.generation) && !self.gate.is_cancelled()
    }

    fn run(&self, links: &[(usize, PlayableLink)]) {
        self.checker
            .check_all(links, &|| self.wanted(), &|index, found| {
                self.record(index, &found)
            });
        let progress = self.gate.progress();
        if !self.wanted() {
            tracing::info!(
                answered = progress.checked,
                of = links.len(),
                elapsed_ms = self.started.elapsed().as_millis(),
                "stream checks cancelled: the video was closed"
            );
            return;
        }
        self.gate.finish();
        (self.tell)(CheckUpdate::SourcesChanged);
        tracing::info!(
            checked = progress.checked,
            working = progress.working,
            elapsed_ms = self.started.elapsed().as_millis(),
            "stream checks finished"
        );
    }

    /// Stores one answer; `false` tells the checker nobody wants more answers.
    fn record(&self, index: usize, found: &LinkHealth) -> bool {
        if let LinkHealth::Broken(reason) = found {
            tracing::debug!(index, %reason, "stream is not playable");
        }
        let progress = self.gate.record(index, found.is_working());
        (self.tell)(CheckUpdate::Progress(progress));
        if !self
            .session
            .set_health(self.generation, index, to_health(found))
        {
            return false;
        }
        if let LinkHealth::Working {
            size_bytes: Some(size),
        } = found
        {
            self.session.set_size(self.generation, index, *size);
        }
        if progress.checked.is_multiple_of(NOTIFY_EVERY) {
            (self.tell)(CheckUpdate::SourcesChanged);
        }
        true
    }
}

fn emit(app: &AppHandle, update: CheckUpdate) {
    let sent = match update {
        CheckUpdate::Progress(progress) => app.emit(SOURCE_CHECK_PROGRESS_EVENT, progress),
        CheckUpdate::SourcesChanged => app.emit(SOURCES_CHANGED_EVENT, ()),
    };
    if let Err(error) = sent {
        tracing::warn!(%error, "could not tell the UI about the stream checks");
    }
}

#[cfg(test)]
#[path = "source_manager_tests.rs"]
mod tests;
