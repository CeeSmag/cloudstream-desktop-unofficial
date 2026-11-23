//! Background watch of the sidecar's Cloudflare exchange folder.
//!
//! - a site the viewer has not allowed stays pending for its specific Verify action;
//! - allowed sites are checked in a hidden window. The viewer can open the active check.
//!
//! Browser fetches are answered by the helper; while no helper runs, they are answered "no window"
//! at once so the sidecar verifies again instead of waiting 90 s.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::exchange::{pending_requests, write_result, Outcome, VerifyRequest};
use crate::fetch_exchange::{pending_fetches, write_fetch_result, FetchOutcome};
use crate::helper_idle::{newest_fetch_file_age, should_stop, IdleInputs};
use crate::helper_protocol::HelperStatus;
use crate::prompt_owners::PromptOwners;
use crate::verify_helper::{HelperLaunch, VerifyHelper};
use crate::watch_passes::{PassDesk, SavedPasses};
use crate::watch_requests::PendingRequest;
use crate::{AllowedHosts, CloudflareEvent};

/// How often the folder is looked at.
const POLL: Duration = Duration::from_millis(250);

/// What the watcher needs.
pub struct WatchConfig {
    /// The exchange folder (`<data>/sidecar/cloudflare`).
    pub dir: PathBuf,
    /// Sites the viewer allowed.
    pub allowed: AllowedHosts,
    /// How to start the verification helper; None when this build has none.
    pub helper: Option<HelperLaunch>,
    /// Where passes are kept for later; None answers every request with a check, as before.
    pub passes: Option<SavedPasses>,
}

/// Stops the background watcher when dropped.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    debug_logging: Arc<AtomicBool>,
    /// Seconds a quiet helper may stay open; 0 = until the app closes.
    helper_idle_secs: Arc<AtomicU64>,
    /// Verified browser windows the helper may keep, read when it starts; 0 = its own default.
    helper_windows: Arc<AtomicUsize>,
    join: Option<JoinHandle<()>>,
    commands: Sender<ViewerAction>,
    next_retry: AtomicU64,
    /// Events for the UI to drain.
    pub events: Receiver<CloudflareEvent>,
}

pub(super) enum ViewerAction {
    Verify(String),
    Retry { failed: String, attempt: String },
    Show(String),
    Dismiss(String),
}

impl WatchHandle {
    /// Approves a pending request.
    pub fn verify_now(&self, id: &str) {
        self.viewer_action(ViewerAction::Verify(id.to_owned()), id);
    }

    /// Starts a fresh attempt for a failed request and returns its new banner ID.
    pub fn retry_check(&self, id: &str) -> String {
        let number = self.next_retry.fetch_add(1, Ordering::Relaxed);
        let attempt = format!("app-retry-{number}");
        self.viewer_action(
            ViewerAction::Retry {
                failed: id.to_owned(),
                attempt: attempt.clone(),
            },
            id,
        );
        attempt
    }

    /// Presents the active check for the viewer.
    pub fn show_check(&self, id: &str) {
        self.viewer_action(ViewerAction::Show(id.to_owned()), id);
    }

    /// Retires the request shown in the banner.
    pub fn dismiss_check(&self, id: &str) {
        self.viewer_action(ViewerAction::Dismiss(id.to_owned()), id);
    }

    fn viewer_action(&self, action: ViewerAction, id: &str) {
        if self.commands.send(action).is_err() {
            tracing::warn!(%id, "the Cloudflare watcher is not running");
        }
    }

    /// Enables or disables verbose debug logging for the verification helper subprocess.
    pub fn set_debug_logging(&self, enabled: bool) {
        self.debug_logging.store(enabled, Ordering::SeqCst);
    }

    /// Changes how long a quiet helper stays open (`None` = until the app closes); applies at once.
    pub fn set_helper_idle(&self, idle: Option<Duration>) {
        self.helper_idle_secs
            .store(idle.map_or(0, |limit| limit.as_secs()), Ordering::SeqCst);
    }

    /// How many verified browser windows the helper keeps. The helper takes it when it starts, so
    /// a change applies the next time it starts.
    pub fn set_helper_windows(&self, windows: usize) {
        self.helper_windows.store(windows, Ordering::SeqCst);
    }

    /// Asks the watcher thread to exit.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(join) = self.join.take() {
            if join.join().is_err() {
                tracing::warn!("the Cloudflare watcher ended with a panic");
            }
        }
    }
}

/// Starts watching `config.dir`.
pub fn spawn_watcher(config: WatchConfig) -> WatchHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let debug_logging = Arc::new(AtomicBool::new(false));
    // Until `set_helper_idle` says otherwise the helper stays open until the app closes.
    let helper_idle_secs = Arc::new(AtomicU64::new(0));
    let helper_windows = Arc::new(AtomicUsize::new(0));
    let (events_tx, events) = mpsc::channel();
    let (commands, commands_rx) = mpsc::channel();
    let loop_flags = WatcherFlags {
        stop: Arc::clone(&stop),
        debug_logging: Arc::clone(&debug_logging),
        helper_idle_secs: Arc::clone(&helper_idle_secs),
        helper_windows: Arc::clone(&helper_windows),
    };
    let join = thread::Builder::new()
        .name("cloudflare-watch".into())
        .spawn(move || run_loop(config, &loop_flags, &events_tx, &commands_rx))
        .map_err(|error| tracing::error!(%error, "could not start the Cloudflare watcher"))
        .ok();
    WatchHandle {
        stop,
        debug_logging,
        helper_idle_secs,
        helper_windows,
        join,
        commands,
        next_retry: AtomicU64::new(1),
        events,
    }
}

struct WatcherFlags {
    stop: Arc<AtomicBool>,
    debug_logging: Arc<AtomicBool>,
    helper_idle_secs: Arc<AtomicU64>,
    helper_windows: Arc<AtomicUsize>,
}

pub(super) struct Watcher<'a> {
    pub(super) dir: PathBuf,
    pub(super) allowed: AllowedHosts,
    pub(super) helper: Option<VerifyHelper>,
    events: &'a Sender<CloudflareEvent>,
    seen_requests: HashSet<String>,
    seen_fetches: HashSet<String>,
    pub(super) requests: HashMap<String, PendingRequest>,
    pub(super) owners: PromptOwners,
    pub(super) promotions: Vec<String>,
    pub(super) passes: Option<PassDesk>,
    helper_idle_secs: Arc<AtomicU64>,
}

fn run_loop(
    config: WatchConfig,
    flags: &WatcherFlags,
    events: &Sender<CloudflareEvent>,
    commands: &Receiver<ViewerAction>,
) {
    if let Err(error) = std::fs::create_dir_all(&config.dir) {
        tracing::warn!(%error, "cannot create the Cloudflare exchange folder");
    }
    let (status_tx, statuses) = mpsc::channel();
    let debug = Arc::clone(&flags.debug_logging);
    let mut watcher = Watcher {
        helper: config.helper.map(|launch| {
            let windows = Arc::clone(&flags.helper_windows);
            VerifyHelper::new(launch, status_tx, Arc::clone(&debug), windows)
        }),
        dir: config.dir,
        allowed: config.allowed,
        events,
        seen_requests: HashSet::new(),
        seen_fetches: HashSet::new(),
        requests: HashMap::new(),
        owners: PromptOwners::default(),
        promotions: Vec::new(),
        passes: config.passes.map(PassDesk::new),
        helper_idle_secs: Arc::clone(&flags.helper_idle_secs),
    };
    while !flags.stop.load(Ordering::SeqCst) {
        watcher.tick(commands, &statuses);
        thread::sleep(POLL);
    }
}

#[cfg(test)]
impl<'a> Watcher<'a> {
    /// A watcher without a helper or thread, for tests that drive it step by step.
    pub(super) fn for_tests(
        dir: PathBuf,
        allowed: AllowedHosts,
        events: &'a Sender<CloudflareEvent>,
        passes: Option<SavedPasses>,
    ) -> Self {
        Watcher {
            dir,
            allowed,
            helper: None,
            events,
            seen_requests: HashSet::new(),
            seen_fetches: HashSet::new(),
            requests: HashMap::new(),
            owners: PromptOwners::default(),
            promotions: Vec::new(),
            passes: passes.map(PassDesk::new),
            helper_idle_secs: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Watcher<'_> {
    fn tick(&mut self, commands: &Receiver<ViewerAction>, statuses: &Receiver<HelperStatus>) {
        for status in statuses.try_iter() {
            self.browser_status(status);
        }
        self.expire_requests();
        for request in pending_requests(&self.dir, &mut self.seen_requests) {
            self.route(request);
        }
        for action in commands.try_iter() {
            self.viewer_action(action);
        }
        self.answer_orphan_fetches();
        for id in std::mem::take(&mut self.promotions) {
            self.promote(&id);
        }
        self.close_helper_if_idle();
    }

    /// Closes a quiet helper (a browser engine of 100-300 MB). Runs on the same thread that routes
    /// requests, so a request cannot arrive in the middle of the decision; the next one starts the
    /// helper again.
    fn close_helper_if_idle(&mut self) {
        let secs = self.helper_idle_secs.load(Ordering::SeqCst);
        let pending_requests = self.requests.len();
        let Some(helper) = self.helper.as_mut() else {
            return;
        };
        if !helper.is_running() {
            return;
        }
        let inputs = IdleInputs {
            ttl: (secs > 0).then(|| Duration::from_secs(secs)),
            since_last_command: helper.idle_for(),
            pending_requests,
            since_last_fetch_file: newest_fetch_file_age(&self.dir),
        };
        if should_stop(&inputs) {
            tracing::info!(
                idle_secs = helper.idle_for().as_secs(),
                "closing the verification helper: idle"
            );
            helper.stop();
        }
    }

    fn answer_orphan_fetches(&mut self) {
        let helper_runs = self.helper.as_mut().is_some_and(VerifyHelper::is_running);
        if helper_runs {
            return;
        }
        for fetch in pending_fetches(&self.dir, &mut self.seen_fetches) {
            let outcome = FetchOutcome::gone("no verification window is open");
            if let Err(error) = write_fetch_result(&self.dir, &fetch.id, &outcome) {
                tracing::warn!(%error, id = %fetch.id, "could not answer a browser fetch");
            }
        }
    }

    pub(super) fn cancel(&mut self, id: String, pid: u32) {
        if let Some(helper) = self.helper.as_mut() {
            if helper.pid() == Some(pid) {
                if let Err(error) = helper.cancel_if_running(id.clone()) {
                    tracing::warn!(%id, %error, "could not cancel the verification window");
                }
            }
        }
    }

    /// Sends `event` to the UI, unless it belongs to a request that waits quietly behind its origin's prompt.
    pub(super) fn emit(&self, event: CloudflareEvent) {
        if event
            .request_id()
            .is_some_and(|id| self.owners.is_quiet(id))
        {
            return;
        }
        if self.events.send(event).is_err() {
            tracing::debug!("the UI no longer listens to Cloudflare events");
        }
    }
}

pub(super) fn answer(dir: &Path, id: &str, outcome: &Outcome) {
    if let Err(error) = write_result(dir, id, outcome) {
        tracing::warn!(%error, %id, "could not answer the sidecar");
    }
}

// The helper publishes the answer before sending its status. If it exits in
// between, recover the status from the answer without replacing the file.
pub(super) fn published_status(
    dir: &Path,
    request: &VerifyRequest,
) -> io::Result<Option<HelperStatus>> {
    if request.standalone {
        return Ok(None);
    }
    let path = dir.join(format!("result-{}.txt", request.id));
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut fields = text.lines();
    let host = request.host.clone();
    let id = request.id.clone();
    match fields.next() {
        Some("ok=true") => Ok(Some(HelperStatus::Passed {
            host,
            id,
            issued: None,
        })),
        Some("ok=false") => {
            let reason = fields
                .find_map(|field| field.strip_prefix("reason="))
                .unwrap_or("the verification did not pass")
                .to_owned();
            Ok(Some(HelperStatus::Failed { host, id, reason }))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid verification result",
        )),
    }
}
