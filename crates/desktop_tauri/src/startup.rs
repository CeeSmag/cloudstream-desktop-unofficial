//! Start-up work shown on the loading screen: loading the installed extensions.

use std::sync::{Mutex, PoisonError};

use app_services::{ExtensionLoadReport, ExtensionService, LoadProgress};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Event the UI listens to so it can show how far start-up has come.
pub const STARTUP_PROGRESS_EVENT: &str = "startup-progress";
/// Event the UI listens to so it can reload providers once extensions finish loading or change.
pub const PROVIDERS_CHANGED_EVENT: &str = "providers-changed";

/// An extension that could not be loaded.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StartupFailure {
    /// Extension file name.
    pub name: String,
    /// Readable reason.
    pub reason: String,
}

/// How far start-up has come; the loading screen shows this.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct StartupStatus {
    /// True once every installed extension was tried and the app can show its home page.
    pub ready: bool,
    /// Extensions already done, loaded or failed.
    pub finished: usize,
    /// Extensions to load in all (0 until the list is known).
    pub total: usize,
    /// The extension being loaded now; empty when none.
    pub current: String,
    /// Extensions that failed; the app still starts.
    pub failures: Vec<StartupFailure>,
}

/// Tauri-managed holder of the start-up status, so a UI that opens late can still read it.
#[derive(Default)]
pub struct StartupTracker {
    status: Mutex<StartupStatus>,
}

impl StartupTracker {
    /// The latest status.
    pub fn snapshot(&self) -> StartupStatus {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Applies `change`, then tells the UI.
    fn update(&self, app: &AppHandle, change: impl FnOnce(&mut StartupStatus)) {
        let status = {
            let mut guard = self.status.lock().unwrap_or_else(PoisonError::into_inner);
            change(&mut guard);
            guard.clone()
        };
        if let Err(error) = app.emit(STARTUP_PROGRESS_EVENT, status) {
            tracing::warn!(%error, "could not tell the UI about start-up progress");
        }
    }
}

/// Returns how far start-up has come.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_startup_status(
    tracker: State<'_, StartupTracker>,
) -> Result<StartupStatus, String> {
    Ok(tracker.snapshot())
}

/// Loads the installed extensions, reporting each step, then marks start-up as finished.
#[tracing::instrument(level = "debug", skip_all)]
pub fn load_extensions(app: &AppHandle, extensions: &ExtensionService) {
    let tracker = app.state::<StartupTracker>();
    let outcome = extensions.load_installed_reporting(&|progress| {
        tracker.update(app, |status| record_progress(status, progress));
    });
    let failures = match outcome {
        Ok(report) => failures_of(&report),
        Err(error) => {
            tracing::warn!(%error, "could not load installed extensions");
            vec![StartupFailure {
                name: "extensions".to_owned(),
                reason: error.to_string(),
            }]
        }
    };
    // Providers first, so the home page already has them when the loading screen goes away.
    if let Err(error) = app.emit(PROVIDERS_CHANGED_EVENT, ()) {
        tracing::warn!(%error, "could not notify the UI about providers");
    }
    tracker.update(app, |status| {
        status.failures = failures;
        status.current.clear();
        status.ready = true;
    });
}

fn failures_of(report: &ExtensionLoadReport) -> Vec<StartupFailure> {
    tracing::info!(
        providers = report.providers.len(),
        failures = report.failures.len(),
        "installed extensions loaded"
    );
    report
        .failures
        .iter()
        .map(|(name, reason)| StartupFailure {
            name: name.clone(),
            reason: reason.clone(),
        })
        .collect()
}

fn record_progress(status: &mut StartupStatus, progress: &LoadProgress) {
    status.finished = progress.finished;
    status.total = progress.total;
    status.current = progress.current.clone();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_up_is_not_ready_until_told() {
        let tracker = StartupTracker::default();
        assert!(!tracker.snapshot().ready);
        assert_eq!(tracker.snapshot().total, 0);
    }

    #[test]
    fn progress_is_copied_into_the_status() {
        let mut status = StartupStatus::default();
        record_progress(
            &mut status,
            &LoadProgress {
                finished: 1,
                total: 3,
                current: "CineStream".to_owned(),
            },
        );
        assert_eq!((status.finished, status.total), (1, 3));
        assert_eq!(status.current, "CineStream");
    }

    #[test]
    fn failures_keep_their_names_and_reasons() {
        let report = ExtensionLoadReport {
            providers: Vec::new(),
            failures: vec![("Bad.cs3".to_owned(), "no classes".to_owned())],
        };
        let failures = failures_of(&report);
        assert_eq!(failures[0].name, "Bad.cs3");
        assert_eq!(failures[0].reason, "no classes");
    }
}
