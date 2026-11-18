//! Background watch of the sidecar's Cloudflare exchange folder.
//!
//! Every request is answered without delay:
//! - a site the viewer has not allowed gets "ask again" at once (the plugin fails fast) and the UI
//!   is told, so it can offer a Verify button;
//! - an allowed site goes to the verification helper, which opens one window per site on the site's
//!   front page (never the request's own address: that was often a video or an API call, and the old
//!   UI opened it in the system browser for every request, which downloaded it again and again).
//!
//! Browser fetches are answered by the helper; while no helper runs, they are answered "no window"
//! at once so the sidecar verifies again instead of waiting 90 s.

use std::collections::HashSet;
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
use crate::AllowedHosts;

/// How often the folder is looked at.
const POLL: Duration = Duration::from_millis(250);

/// Something the UI should show about a site's Cloudflare check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudflareEvent {
    /// The site needs a check and the viewer has not allowed it yet (offer Verify).
    NeedsPermission {
        /// The site.
        host: String,
    },
    /// A verification window is open for the site.
    Verifying {
        /// The site.
        host: String,
    },
    /// The check was passed.
    Passed {
        /// The site.
        host: String,
    },
    /// The check was not passed.
    Failed {
        /// The site.
        host: String,
        /// Why, for the viewer.
        reason: String,
    },
}

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
    join: Option<JoinHandle<()>>,
    commands: Sender<String>,
    /// Events for the UI to drain.
    pub events: Receiver<CloudflareEvent>,
}

impl WatchHandle {
    /// Opens the verification window for `host` now (the viewer pressed Verify).
    pub fn verify_now(&self, host: &str) {
        if self.commands.send(host.to_owned()).is_err() {
            tracing::warn!(%host, "the Cloudflare watcher is not running");
        }
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
    let (events_tx, events) = mpsc::channel();
    let (commands, commands_rx) = mpsc::channel();
    let stop_flag = Arc::clone(&stop);
    let join = thread::Builder::new()
        .name("cloudflare-watch".into())
        .spawn(move || run_loop(config, &stop_flag, &events_tx, &commands_rx))
        .map_err(|error| tracing::error!(%error, "could not start the Cloudflare watcher"))
        .ok();
    WatchHandle {
        stop,
        join,
        commands,
        events,
    }
}

struct Watcher<'a> {
    dir: PathBuf,
    allowed: AllowedHosts,
    helper: Option<VerifyHelper>,
    events: &'a Sender<CloudflareEvent>,
    seen_requests: HashSet<String>,
    seen_fetches: HashSet<String>,
    next_app_request: u64,
}

fn run_loop(
    config: WatchConfig,
    stop: &AtomicBool,
    events: &Sender<CloudflareEvent>,
    commands: &Receiver<String>,
) {
    if let Err(error) = std::fs::create_dir_all(&config.dir) {
        tracing::warn!(%error, "cannot create the Cloudflare exchange folder");
    }
    let (status_tx, statuses) = mpsc::channel();
    let mut watcher = Watcher {
        helper: config
            .helper
            .map(|launch| VerifyHelper::new(launch, status_tx)),
        dir: config.dir,
        allowed: config.allowed,
        events,
        seen_requests: HashSet::new(),
        seen_fetches: HashSet::new(),
        next_app_request: 0,
    };
    while !stop.load(Ordering::SeqCst) {
        watcher.tick(commands, &statuses);
        thread::sleep(POLL);
    }
}

impl Watcher<'_> {
    fn tick(&mut self, commands: &Receiver<String>, statuses: &Receiver<HelperStatus>) {
        for request in pending_requests(&self.dir, &mut self.seen_requests) {
            self.route(request);
        }
        for host in commands.try_iter() {
            self.verify_now(&host);
        }
        for status in statuses.try_iter() {
            self.emit(event_for(status));
        }
        self.answer_orphan_fetches();
    }

    fn route(&mut self, request: VerifyRequest) {
        tracing::info!(host = %request.host, id = %request.id, "a site needs Cloudflare verification");
        if !self.allowed.is_allowed(&request.host) {
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
            Some(helper) => helper.send(&HelperCommand::Verify(request.clone())),
            None => Err(std::io::Error::other(
                "this build has no verification window",
            )),
        };
        match sent {
            Ok(()) => self.emit(CloudflareEvent::Verifying { host }),
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
            url: format!("https://{host}/"),
        };
        self.allowed.allow(host);
        self.route(request);
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

fn event_for(status: HelperStatus) -> CloudflareEvent {
    match status {
        HelperStatus::WindowOpen { host } => CloudflareEvent::Verifying { host },
        HelperStatus::Passed { host } => CloudflareEvent::Passed { host },
        HelperStatus::Failed { host, reason } => CloudflareEvent::Failed { host, reason },
    }
}

fn answer(dir: &Path, id: &str, outcome: &Outcome) {
    if let Err(error) = write_result(dir, id, outcome) {
        tracing::warn!(%error, %id, "could not answer the sidecar");
    }
}
