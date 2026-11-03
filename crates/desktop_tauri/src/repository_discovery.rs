//! Collects repositories that extensions add on their own (the "Mega" plugin adds every known
//! repository each time it loads) and saves them, telling the UI when the list grows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use app_services::{ExtensionService, RepositoryService};
use core_model::DiscoveredRepository;
use tauri::{AppHandle, Emitter};

/// Event the UI listens to so it can reload the repository list.
pub const REPOSITORIES_CHANGED_EVENT: &str = "repositories-changed";

/// Pause between asks of the runtime.
const POLL_EVERY: Duration = Duration::from_secs(2);
/// Give up this long after starting, whatever happens.
const WATCH_AT_MOST: Duration = Duration::from_secs(300);
/// After something was found, stop once this many asks in a row bring nothing new.
const QUIET_POLLS_BEFORE_STOP: u32 = 15;
/// Stop after this many failed asks in a row.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

static WATCHING: AtomicBool = AtomicBool::new(false);

/// Starts a background watcher unless one is already running.
#[tracing::instrument(level = "debug", skip_all)]
pub fn watch(handle: AppHandle, extensions: ExtensionService, repositories: RepositoryService) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        tracing::debug!("repository watcher already running");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("repository-discovery".to_owned())
        .spawn(move || {
            run(&handle, &extensions, &repositories);
            WATCHING.store(false, Ordering::SeqCst);
        });
    if let Err(error) = spawned {
        WATCHING.store(false, Ordering::SeqCst);
        tracing::error!(%error, "cannot start the repository watcher");
    }
}

/// How the watch is going; decides when to stop.
#[derive(Default)]
struct Progress {
    quiet_polls: u32,
    failures: u32,
    found_any: bool,
}

impl Progress {
    fn found_something(&mut self) {
        self.failures = 0;
        self.quiet_polls = 0;
        self.found_any = true;
    }

    fn found_nothing(&mut self) {
        self.failures = 0;
        self.quiet_polls += 1;
    }

    fn failed(&mut self) {
        self.failures += 1;
    }

    fn should_stop(&self) -> bool {
        self.failures >= MAX_CONSECUTIVE_FAILURES
            || (self.found_any && self.quiet_polls >= QUIET_POLLS_BEFORE_STOP)
    }
}

fn run(handle: &AppHandle, extensions: &ExtensionService, repositories: &RepositoryService) {
    let started = Instant::now();
    let mut progress = Progress::default();
    while started.elapsed() < WATCH_AT_MOST && !progress.should_stop() {
        std::thread::sleep(POLL_EVERY);
        match extensions.take_added_repositories() {
            Ok(found) if found.is_empty() => progress.found_nothing(),
            Ok(found) => {
                progress.found_something();
                import(handle, repositories, &found);
            }
            Err(error) => {
                progress.failed();
                tracing::warn!(%error, failures = progress.failures, "could not ask extensions for new repositories");
            }
        }
    }
    tracing::info!(
        found_any = progress.found_any,
        seconds = started.elapsed().as_secs(),
        "repository watcher finished"
    );
}

fn import(handle: &AppHandle, repositories: &RepositoryService, found: &[DiscoveredRepository]) {
    match repositories.import_discovered(found) {
        Ok(0) => {}
        Ok(_) => {
            if let Err(error) = handle.emit(REPOSITORIES_CHANGED_EVENT, ()) {
                tracing::warn!(%error, "could not tell the UI about new repositories");
            }
        }
        Err(error) => tracing::warn!(%error, "could not save repositories added by extensions"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_watching_until_something_was_found_and_then_went_quiet() {
        let mut progress = Progress::default();
        for _ in 0..QUIET_POLLS_BEFORE_STOP * 2 {
            progress.found_nothing();
        }
        assert!(!progress.should_stop(), "nothing found yet: keep waiting");
        progress.found_something();
        for _ in 0..QUIET_POLLS_BEFORE_STOP {
            progress.found_nothing();
        }
        assert!(progress.should_stop());
    }

    #[test]
    fn stops_after_repeated_failures_and_forgives_a_single_one() {
        let mut progress = Progress::default();
        progress.failed();
        progress.found_nothing();
        assert_eq!(progress.failures, 0);
        for _ in 0..MAX_CONSECUTIVE_FAILURES {
            progress.failed();
        }
        assert!(progress.should_stop());
    }
}
