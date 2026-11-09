//! Answers the sidecar's Cloudflare requests for as long as the app runs: verification requests (open a
//! window, once the viewer has allowed that site) and browser fetches (make a request in a window that
//! already passed the check).
//!
//! Everything is logged to `cloudflare-logs.txt` and, at info level, to the main log.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_services::ExtensionService;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::cloudflare_exchange::{
    clear_folder, fresh_files, pending_requests, write_named, write_result, Outcome, VerifyRequest,
};
use crate::cloudflare_fetch::{
    format_fetch_result, parse_fetch_request, run_fetch, FetchOutcome, FetchRequest,
};
use crate::cloudflare_gate::{provider_of_host, Gate};
use crate::cloudflare_open::{close_kept, find};
use crate::cloudflare_window::{label_for, short, verify};

/// How often the exchange folder is looked at.
const WATCH_PAUSE: Duration = Duration::from_millis(250);
/// How long one request made in the browser may take.
const FETCH_LIMIT: Duration = Duration::from_secs(90);
/// A window nobody has used for this long is closed.
const WINDOW_IDLE: Duration = Duration::from_secs(20 * 60);
/// The page event that tells the UI a site needs the viewer's go-ahead.
pub const NEEDED_EVENT: &str = "cloudflare-needed";

/// Where the verification keeps its files.
pub struct Folders {
    /// Where the sidecar's requests and the app's answers are exchanged.
    pub exchange: PathBuf,
    /// The verification windows' own browser profile.
    pub browser_profile: PathBuf,
}

/// What the page is told when a site needs the viewer's go-ahead.
#[derive(Debug, Clone, Serialize)]
struct NeededPayload {
    host: String,
    provider: Option<String>,
}

/// Starts answering for the data folder `data_dir`.
pub fn start(app: &AppHandle, data_dir: &Path, extensions: &ExtensionService, gate: Arc<Gate>) {
    let folders = Folders {
        exchange: data_dir.join("sidecar").join("cloudflare"),
        browser_profile: data_dir.join("cloudflare_browser"),
    };
    clear_folder(&folders.exchange);
    tracing::info!(
        exchange = %folders.exchange.display(),
        profile = %folders.browser_profile.display(),
        "Cloudflare verification watcher starting; windows open only for sites the viewer allowed"
    );
    let (app, extensions) = (app.clone(), extensions.clone());
    let spawned = std::thread::Builder::new()
        .name("cloudflare-watch".to_owned())
        .spawn(move || watch(&app, &folders, (&extensions, &gate)));
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start the Cloudflare verification watcher: sites behind Cloudflare will not load");
    }
}

fn watch(app: &AppHandle, folders: &Folders, helpers: (&ExtensionService, &Gate)) {
    let (extensions, gate) = helpers;
    let mut seen = std::collections::HashSet::new();
    let mut last_used: HashMap<String, Instant> = HashMap::new();
    loop {
        for request in pending_requests(&folders.exchange, &mut seen) {
            answer_verification(app, folders, (extensions, gate), &request);
            last_used.insert(label_for(&request.host), Instant::now());
        }
        for request in pending_fetches(&folders.exchange, &mut seen) {
            answer_fetch(app, folders, &request);
            last_used.insert(label_for(&request.host), Instant::now());
        }
        close_idle_windows(app, &mut last_used);
        std::thread::sleep(WATCH_PAUSE);
    }
}

fn pending_fetches(dir: &Path, seen: &mut std::collections::HashSet<String>) -> Vec<FetchRequest> {
    fresh_files(dir, "fetch-")
        .into_iter()
        .filter_map(|(text, _)| parse_fetch_request(&text))
        .filter(|request| seen.insert(request.id.clone()))
        .collect()
}

/// A verification request: open the window if the viewer has allowed the site, else ask the page for the go-ahead.
fn answer_verification(
    app: &AppHandle,
    folders: &Folders,
    helpers: (&ExtensionService, &Gate),
    request: &VerifyRequest,
) {
    let (extensions, gate) = helpers;
    tracing::info!(id = %request.id, host = %request.host, page = %short(&request.url), "a site needs Cloudflare verification; request received from the sidecar");
    let started = Instant::now();
    let outcome = if gate.is_allowed(&request.host) {
        verify(app, (extensions, &folders.browser_profile), request)
    } else {
        ask_for_go_ahead(app, extensions, &request.host)
    };
    log_outcome(request, &outcome, started.elapsed());
    match write_result(&folders.exchange, &request.id, &outcome) {
        Ok(path) => {
            tracing::info!(id = %request.id, file = %path.display(), "answer handed to the sidecar")
        }
        Err(error) => {
            tracing::error!(id = %request.id, %error, "cannot hand the verification answer to the sidecar; the request will time out")
        }
    }
}

/// The viewer has not asked for this site: no window now; the page is told, and shows a Verify button.
fn ask_for_go_ahead(app: &AppHandle, extensions: &ExtensionService, host: &str) -> Outcome {
    let provider = provider_of_host(&extensions.providers(), host);
    tracing::info!(%host, ?provider, "not opening a window: the viewer has not asked for this site yet; telling the page (Verify button)");
    let payload = NeededPayload {
        host: host.to_owned(),
        provider,
    };
    if let Err(error) = app.emit(NEEDED_EVENT, payload) {
        tracing::warn!(%error, "cannot tell the page that a site needs verification");
    }
    Outcome::Waiting(format!(
        "{host} needs a quick Cloudflare check; press Verify on the Home page"
    ))
}

fn log_outcome(request: &VerifyRequest, outcome: &Outcome, took: Duration) {
    match outcome {
        Outcome::Passed {
            cookies,
            user_agent,
        } => {
            let names: Vec<&str> = cookies.iter().map(|(name, _)| name.as_str()).collect();
            tracing::info!(host = %request.host, ?names, %user_agent, took_s = took.as_secs(), "Cloudflare verification PASSED");
        }
        Outcome::Declined(reason) => {
            tracing::warn!(host = %request.host, %reason, took_s = took.as_secs(), "Cloudflare verification NOT done");
        }
        Outcome::Waiting(reason) => {
            tracing::info!(host = %request.host, %reason, "Cloudflare verification waiting for the viewer's go-ahead");
        }
    }
}

/// A browser fetch: run it in the site's window, if there is one.
fn answer_fetch(app: &AppHandle, folders: &Folders, request: &FetchRequest) {
    let started = Instant::now();
    let label = label_for(&request.host);
    let outcome = match find(app, &label) {
        Some(surface) => run_fetch(surface.view(), request, FETCH_LIMIT),
        None => FetchOutcome::gone(format!("no browser window for {}", request.host)),
    };
    log_fetch(request, &outcome, started.elapsed());
    let name = format!("fetchresult-{}", request.id);
    if let Err(error) = write_named(&folders.exchange, &name, &format_fetch_result(&outcome)) {
        tracing::error!(id = %request.id, %error, "cannot hand the browser's answer to the sidecar; the request will time out");
    }
}

fn log_fetch(request: &FetchRequest, outcome: &FetchOutcome, took: Duration) {
    let page = short(&request.url);
    match outcome {
        FetchOutcome::Done {
            status,
            body_base64,
            ..
        } => {
            tracing::info!(method = %request.method, %page, status, body_chars = body_base64.len(), took_ms = took.as_millis() as u64, "browser fetch done");
        }
        FetchOutcome::Failed { reason, gone } => {
            tracing::warn!(method = %request.method, %page, %reason, gone, took_ms = took.as_millis() as u64, "browser fetch FAILED");
        }
    }
}

/// Closes the hidden windows nobody has used for a long time.
fn close_idle_windows(app: &AppHandle, last_used: &mut HashMap<String, Instant>) {
    last_used.retain(|label, used| {
        let idle = used.elapsed() >= WINDOW_IDLE;
        if idle {
            tracing::info!(%label, "closing a Cloudflare check nobody has used for a while");
            close_kept(app, label);
        }
        !idle
    });
}
