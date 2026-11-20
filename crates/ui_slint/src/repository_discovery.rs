//! Single-worker background coordinator that polls for repositories registered asynchronously by
//! aggregator plugins (such as Mega) without global mutable state.

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::Instant;

use core_model::RepositoryEntry;

use crate::discovery_progress::{DiscoveryConfig, WatchProgress};

/// Outcome of one poll-and-import cycle.
#[derive(Debug)]
pub(crate) enum PollOutcome {
    Empty,
    Found {
        added: usize,
        repositories: Vec<RepositoryEntry>,
    },
}

/// Event emitted from the discovery worker to the UI.
#[derive(Debug)]
pub(crate) enum DiscoveryUpdate {
    Added {
        added: usize,
        repositories: Vec<RepositoryEntry>,
    },
    ReloadCatalog,
}

#[derive(Default)]
struct SharedState {
    running: bool,
    stopped: bool,
    trigger_generation: u64,
}

struct DiscoveryShared {
    state: Mutex<SharedState>,
    wake: Condvar,
}

impl DiscoveryShared {
    fn lock_state(&self) -> std::sync::MutexGuard<'_, SharedState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Coordinates at most one background repository-discovery worker per UI session.
pub(crate) struct RepositoryDiscovery {
    shared: Arc<DiscoveryShared>,
    config: DiscoveryConfig,
}

impl Default for RepositoryDiscovery {
    fn default() -> Self {
        Self::with_config(DiscoveryConfig::default())
    }
}

impl Drop for RepositoryDiscovery {
    fn drop(&mut self) {
        self.stop();
    }
}

impl RepositoryDiscovery {
    pub(crate) fn with_config(config: DiscoveryConfig) -> Self {
        Self {
            shared: Arc::new(DiscoveryShared {
                state: Mutex::new(SharedState::default()),
                wake: Condvar::new(),
            }),
            config,
        }
    }

    /// Stops any active watcher and prevents future spawns.
    pub(crate) fn stop(&self) {
        let mut state = self.shared.lock_state();
        state.stopped = true;
        self.shared.wake.notify_all();
    }

    /// Starts the worker, or wakes and extends the active one's quiet window if already running.
    pub(crate) fn trigger_with<P, E>(&self, poll: P, emit: E)
    where
        P: FnMut() -> Result<PollOutcome, String> + Send + 'static,
        E: FnMut(DiscoveryUpdate) + Send + 'static,
    {
        let Some(seen_gen) = self.claim_or_wake() else {
            return;
        };
        let worker = WorkerLoop {
            shared: Arc::clone(&self.shared),
            config: self.config,
            seen_gen,
            window_started: Instant::now(),
            progress: WatchProgress::default(),
            poll,
            emit,
        };
        let spawned = thread::Builder::new()
            .name("repository-discovery".to_owned())
            .spawn(move || worker.run());
        if let Err(error) = spawned {
            let mut state = self.shared.lock_state();
            state.running = false;
            self.shared.wake.notify_all();
            tracing::error!(%error, "cannot start the repository discovery watcher");
        }
    }

    fn claim_or_wake(&self) -> Option<u64> {
        let mut state = self.shared.lock_state();
        if state.stopped {
            return None;
        }
        state.trigger_generation = state.trigger_generation.wrapping_add(1);
        if state.running {
            self.shared.wake.notify_all();
            return None;
        }
        state.running = true;
        Some(state.trigger_generation)
    }

    #[cfg(test)]
    pub(crate) fn is_running(&self) -> bool {
        self.shared.lock_state().running
    }
}

enum WaitStep {
    Poll,
    Retriggered(u64),
    Stop,
}

struct WorkerLoop<P, E> {
    shared: Arc<DiscoveryShared>,
    config: DiscoveryConfig,
    seen_gen: u64,
    window_started: Instant,
    progress: WatchProgress,
    poll: P,
    emit: E,
}

impl<P, E> WorkerLoop<P, E>
where
    P: FnMut() -> Result<PollOutcome, String>,
    E: FnMut(DiscoveryUpdate),
{
    fn run(mut self) {
        loop {
            match self.wait_next() {
                WaitStep::Stop => break,
                WaitStep::Retriggered(next_gen) => {
                    self.reset_window(next_gen);
                    continue;
                }
                WaitStep::Poll => {}
            }
            self.poll_once();
            if self.ready_to_finish() {
                if let Some(next_gen) = self.finish_or_retrigger() {
                    self.reset_window(next_gen);
                    continue;
                }
                if self.progress.take_final_catalog_reload() {
                    (self.emit)(DiscoveryUpdate::ReloadCatalog);
                }
                break;
            }
        }
        tracing::info!(
            found_any = self.progress.found_any,
            seconds = self.window_started.elapsed().as_secs(),
            "repository watcher finished"
        );
    }

    fn reset_window(&mut self, next_gen: u64) {
        self.seen_gen = next_gen;
        self.window_started = Instant::now();
        self.progress.reset_for_trigger();
    }

    fn ready_to_finish(&self) -> bool {
        self.progress.should_stop(&self.config)
            || self.window_started.elapsed() >= self.config.watch_at_most
    }

    fn wait_next(&self) -> WaitStep {
        let guard = self.shared.lock_state();
        let seen_gen = self.seen_gen;
        let (mut guard, _) = self
            .shared
            .wake
            .wait_timeout_while(guard, self.config.poll_every, |s| {
                !s.stopped && s.trigger_generation == seen_gen
            })
            .unwrap_or_else(PoisonError::into_inner);
        if guard.stopped {
            guard.running = false;
            return WaitStep::Stop;
        }
        if guard.trigger_generation != seen_gen {
            return WaitStep::Retriggered(guard.trigger_generation);
        }
        WaitStep::Poll
    }

    fn finish_or_retrigger(&self) -> Option<u64> {
        let mut guard = self.shared.lock_state();
        if guard.stopped || guard.trigger_generation == self.seen_gen {
            guard.running = false;
            return None;
        }
        Some(guard.trigger_generation)
    }

    fn poll_once(&mut self) {
        match (self.poll)() {
            Ok(PollOutcome::Empty) => {
                self.progress.record_empty();
                if self
                    .progress
                    .take_catalog_reload(self.config.settle_polls_before_catalog)
                {
                    (self.emit)(DiscoveryUpdate::ReloadCatalog);
                }
            }
            Ok(PollOutcome::Found {
                added,
                repositories,
            }) => {
                self.progress.record_discovered(added);
                if added > 0 {
                    (self.emit)(DiscoveryUpdate::Added {
                        added,
                        repositories,
                    });
                }
            }
            Err(error) => {
                self.progress.record_failure();
                tracing::warn!(
                    %error,
                    failures = self.progress.failures,
                    "could not ask extensions for new repositories"
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "repository_discovery_tests.rs"]
mod tests;
