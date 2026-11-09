//! Closing the app: the closer ends everything the app started, in order, and logs every step.
//!
//! Why it exists: the process exits directly when the last window closes (no `Drop` runs), and a hidden
//! window (the kept Cloudflare check) keeps the app alive after the main window is gone. So the closer
//! runs on the main window's close request and again on exit (once only), closes the other windows and
//! views, stops the player, ends the sidecar's whole process tree, and a watchdog forces the exit if a
//! step hangs. Each step is logged with its time (`app-logs.txt`, and the main log).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

use crate::app_state::AppState;

/// How long closing may take before the process is ended anyway.
const GRACE: Duration = Duration::from_secs(10);
/// The label of the app's own window.
const MAIN: &str = "main";

/// Runs the closing steps once; later callers wait for the first to finish.
#[derive(Default)]
pub struct Closer {
    started: AtomicBool,
    finished: AtomicBool,
}

/// One closing step: a name for the log and the work.
pub type Step<'a> = (&'a str, &'a dyn Fn());

impl Closer {
    /// Runs `steps` unless another caller already does; then waits (up to `GRACE`) for that caller.
    /// Returns whether this call did the work.
    pub fn run_once(&self, why: &str, steps: &[Step<'_>]) -> bool {
        if self.started.swap(true, Ordering::SeqCst) {
            tracing::info!(%why, "closing is already under way; waiting for it to finish");
            self.wait_finished();
            return false;
        }
        let started = Instant::now();
        tracing::info!(%why, steps = steps.len(), "closing the app: started");
        for (name, work) in steps {
            let step_started = Instant::now();
            tracing::info!(step = *name, "closing: step started");
            work();
            tracing::info!(
                step = *name,
                took_ms = elapsed_ms(step_started),
                "closing: step done"
            );
        }
        self.finished.store(true, Ordering::SeqCst);
        tracing::info!(
            took_ms = elapsed_ms(started),
            "closing the app: every step done"
        );
        true
    }

    fn wait_finished(&self) {
        let waited = Instant::now();
        while !self.finished.load(Ordering::SeqCst) {
            if waited.elapsed() >= GRACE {
                tracing::error!(
                    "closing did not finish in time; leaving without waiting any longer"
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Handles the events that close the app. Wired in `main` as the run callback.
pub fn handle(app: &AppHandle, event: &RunEvent) {
    match event {
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } if label == MAIN => {
            api.prevent_close();
            close_in_background(app.clone());
        }
        RunEvent::ExitRequested { .. } => close_everything(app, "exit requested"),
        RunEvent::Exit => close_everything(app, "exit"),
        _ => {}
    }
}

/// The main window was closed: it disappears at once, the rest is closed off the window thread.
fn close_in_background(app: AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        if let Err(error) = window.hide() {
            tracing::warn!(%error, "cannot hide the main window while closing");
        }
    }
    let spawned = std::thread::Builder::new()
        .name("closer".to_owned())
        .spawn({
            let app = app.clone();
            move || {
                close_everything(&app, "the main window was closed");
                app.exit(0);
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start the closer thread; closing on this thread");
        close_everything(&app, "closer thread unavailable");
        app.exit(0);
    }
}

/// Ends everything the app started. Safe to call many times.
pub fn close_everything(app: &AppHandle, why: &str) {
    let Some(closer) = app.try_state::<std::sync::Arc<Closer>>() else {
        tracing::error!(%why, "no closer is registered; nothing is stopped by hand");
        return;
    };
    start_watchdog();
    let windows = || close_other_windows(app);
    let player = || stop_player(app);
    let sidecar = || stop_sidecar(app);
    closer.run_once(
        why,
        &[
            ("close the check windows and views", &windows),
            ("stop the player", &player),
            ("stop the sidecar", &sidecar),
        ],
    );
}

/// If closing hangs, the process is ended after `GRACE` so nothing is left running.
fn start_watchdog() {
    static ARMED: AtomicBool = AtomicBool::new(false);
    if ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("closer-watchdog".to_owned())
        .spawn(|| {
            std::thread::sleep(GRACE);
            tracing::error!(
                grace_s = GRACE.as_secs(),
                "closing is stuck; ending the process now"
            );
            std::process::exit(0);
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot start the closing watchdog; a stuck step would hang the exit");
    }
}

/// Every window and view except the main one: the hidden Cloudflare check would keep the app alive.
fn close_other_windows(app: &AppHandle) {
    let mut closed = 0;
    for (label, window) in app.webview_windows() {
        if label != MAIN {
            report("window", &label, window.destroy());
            closed += 1;
        }
    }
    for (label, view) in app.webviews() {
        if label != MAIN && view.window().label() == MAIN {
            report("view", &label, view.close());
            closed += 1;
        }
    }
    tracing::info!(closed, "closing: windows and views closed");
}

fn report(kind: &str, label: &str, result: tauri::Result<()>) {
    match result {
        Ok(()) => tracing::info!(%kind, %label, "closing: closed"),
        Err(error) => tracing::warn!(%kind, %label, %error, "closing: cannot close"),
    }
}

fn stop_player(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        tracing::info!("closing: no player state to stop");
        return;
    };
    let services = state.read();
    match services.playback.as_ref() {
        Some(playback) => match playback.stop() {
            Ok(()) => tracing::info!("closing: the player is stopped"),
            Err(error) => tracing::warn!(%error, "closing: cannot stop the player"),
        },
        None => tracing::info!("closing: there is no player"),
    }
}

fn stop_sidecar(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        tracing::info!("closing: no extension state, no sidecar to stop");
        return;
    };
    state.read().extensions.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    #[test]
    fn steps_run_in_order_exactly_once_however_many_callers() {
        let closer = Arc::new(Closer::default());
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let (closer, log) = (Arc::clone(&closer), Arc::clone(&log));
                std::thread::spawn(move || {
                    let first = || log.lock().expect("log").push("first");
                    let second = || {
                        std::thread::sleep(Duration::from_millis(100));
                        log.lock().expect("log").push("second");
                    };
                    closer.run_once("test", &[("one", &first), ("two", &second)]);
                    // Every caller, first or not, returns only after all steps are done.
                    assert_eq!(
                        log.lock().expect("log").len(),
                        2,
                        "returned before closing finished"
                    );
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("worker");
        }
        assert_eq!(*log.lock().expect("log"), vec!["first", "second"]);
    }

    #[test]
    fn only_one_caller_does_the_work() {
        let closer = Closer::default();
        let count = AtomicUsize::new(0);
        let work = || {
            count.fetch_add(1, Ordering::SeqCst);
        };
        assert!(closer.run_once("a", &[("w", &work)]));
        assert!(!closer.run_once("b", &[("w", &work)]));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
