//! Background watch of the sidecar's Cloudflare exchange folder.
//!
//! Every request is answered without delay:
//! - a site the viewer has not allowed gets "ask again" at once (the plugin fails fast) and the UI
//!   is told, so it can offer a Verify button;
//! - allowed sites are checked in a hidden window. The viewer can open the active check.
//!
//! Browser fetches are answered by the helper; while no helper runs, they are answered "no window"
//! at once so the sidecar verifies again instead of waiting 90 s.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::exchange::{pending_requests, write_result, Outcome, VerifyRequest};
use crate::fetch_exchange::{pending_fetches, write_fetch_result, FetchOutcome};
use crate::helper_protocol::{HelperCommand, HelperStatus};
use crate::verify_helper::{HelperLaunch, VerifyHelper};
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
}

/// Stops the background watcher when dropped.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    debug_logging: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    commands: Sender<ViewerAction>,
    /// Events for the UI to drain.
    pub events: Receiver<CloudflareEvent>,
}

enum ViewerAction {
    Verify(String),
    Show { host: String, id: String },
}

impl WatchHandle {
    /// Opens the verification window for `host` now (the viewer pressed Verify).
    pub fn verify_now(&self, host: &str) {
        if self
            .commands
            .send(ViewerAction::Verify(host.to_owned()))
            .is_err()
        {
            tracing::warn!(%host, "the Cloudflare watcher is not running");
        }
    }

    /// Presents the active check for the viewer.
    pub fn show_check(&self, host: &str, id: &str) {
        if self
            .commands
            .send(ViewerAction::Show {
                host: host.to_owned(),
                id: id.to_owned(),
            })
            .is_err()
        {
            tracing::warn!(%host, "the Cloudflare watcher is not running");
        }
    }

    /// Enables or disables verbose debug logging for the verification helper subprocess.
    pub fn set_debug_logging(&self, enabled: bool) {
        self.debug_logging.store(enabled, Ordering::SeqCst);
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
    let (events_tx, events) = mpsc::channel();
    let (commands, commands_rx) = mpsc::channel();
    let loop_flags = WatcherFlags {
        stop: Arc::clone(&stop),
        debug_logging: Arc::clone(&debug_logging),
    };
    let join = thread::Builder::new()
        .name("cloudflare-watch".into())
        .spawn(move || run_loop(config, &loop_flags, &events_tx, &commands_rx))
        .map_err(|error| tracing::error!(%error, "could not start the Cloudflare watcher"))
        .ok();
    WatchHandle {
        stop,
        debug_logging,
        join,
        commands,
        events,
    }
}

struct WatcherFlags {
    stop: Arc<AtomicBool>,
    debug_logging: Arc<AtomicBool>,
}

struct Watcher<'a> {
    dir: PathBuf,
    allowed: AllowedHosts,
    helper: Option<VerifyHelper>,
    events: &'a Sender<CloudflareEvent>,
    seen_requests: HashSet<String>,
    seen_fetches: HashSet<String>,
    challenge_urls: HashMap<String, String>,
    next_app_request: u64,
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
        helper: config
            .helper
            .map(|launch| VerifyHelper::new(launch, status_tx, Arc::clone(&debug))),
        dir: config.dir,
        allowed: config.allowed,
        events,
        seen_requests: HashSet::new(),
        seen_fetches: HashSet::new(),
        challenge_urls: HashMap::new(),
        next_app_request: 0,
    };
    while !flags.stop.load(Ordering::SeqCst) {
        watcher.tick(commands, &statuses);
        thread::sleep(POLL);
    }
}

impl Watcher<'_> {
    fn tick(&mut self, commands: &Receiver<ViewerAction>, statuses: &Receiver<HelperStatus>) {
        for request in pending_requests(&self.dir, &mut self.seen_requests) {
            self.route(request, false);
        }
        for action in commands.try_iter() {
            match action {
                ViewerAction::Verify(host) => self.verify_now(&host),
                ViewerAction::Show { host, id } => {
                    if let Some(helper) = self.helper.as_mut() {
                        if let Err(error) = helper.show_if_running(&host, &id) {
                            self.emit(CloudflareEvent::Failed {
                                host,
                                reason: error.to_string(),
                            });
                        }
                    }
                }
            }
        }
        for status in statuses.try_iter() {
            if let HelperStatus::Passed { host } = &status {
                self.challenge_urls.remove(host);
            }
            self.emit(status.into());
        }
        self.answer_orphan_fetches();
    }

    fn route(&mut self, request: VerifyRequest, present: bool) {
        tracing::info!(host = %request.host, id = %request.id, "a site needs Cloudflare verification");
        self.challenge_urls
            .insert(request.host.clone(), request.url.clone());
        let allowed = self.allowed.is_allowed(&request.host);
        if !allowed {
            let reason = format!(
                "{} needs a quick check; press Verify in CloudStream",
                request.host
            );
            answer(&self.dir, &request.id, &Outcome::Waiting(reason));
            self.emit(CloudflareEvent::NeedsPermission { host: request.host });
            return;
        }
        let host = request.host.clone();
        let sent = match self.helper.as_mut() {
            Some(helper) => helper.send(&if present {
                HelperCommand::Present(request.clone())
            } else {
                HelperCommand::Verify(request.clone())
            }),
            None => Err(std::io::Error::other(
                "this build has no verification window",
            )),
        };
        match sent {
            Ok(()) => tracing::debug!(%host, id = %request.id, "verification requested"),
            Err(error) => {
                tracing::warn!(%host, %error, "cannot open the verification window");
                let reason = format!("the verification window could not open: {error}");
                answer(&self.dir, &request.id, &Outcome::Declined(reason.clone()));
                self.emit(CloudflareEvent::Failed { host, reason });
            }
        }
    }

    fn verify_now(&mut self, host: &str) {
        self.next_app_request += 1;
        let request = VerifyRequest {
            id: format!("app-{}", self.next_app_request),
            host: host.to_owned(),
            url: self
                .challenge_urls
                .get(host)
                .cloned()
                .unwrap_or_else(|| format!("https://{host}/")),
        };
        self.allowed.allow(host);
        self.route(request, true);
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

    fn emit(&self, event: CloudflareEvent) {
        if self.events.send(event).is_err() {
            tracing::debug!("the UI no longer listens to Cloudflare events");
        }
    }
}

fn answer(dir: &Path, id: &str, outcome: &Outcome) {
    if let Err(error) = write_result(dir, id, outcome) {
        tracing::warn!(%error, %id, "could not answer the sidecar");
    }
}
