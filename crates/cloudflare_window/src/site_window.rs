//! One site's verification window: the check, then browser fetches for the sidecar.

#[path = "site_window_check.rs"]
mod site_window_check;

use std::time::{Duration, Instant};

use cloudflare_bridge::{FetchRequest, Outcome};
use cloudflare_window::verify_open_url::page_to_open;
use cloudflare_window::verify_progress::VerifyProgress;
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::Window;
use wry::{WebContext, WebView};

use crate::browser_view::open_site_view;
use crate::helper_events::HelperEvent;
use crate::outbox::Outbox;
use crate::page_fetches::PageFetches;

/// How often a waiting window says what it sees.
pub(super) const REPORT_EVERY: Duration = Duration::from_secs(5);
/// Keep the window hidden this long so Managed challenges can auto-pass without a popup.
pub(super) const SHOW_AFTER: Duration = Duration::from_secs(3);

pub(super) enum Phase {
    Verifying(VerifyProgress),
    Passed,
}

/// A browser window on one site.
pub(crate) struct SiteWindow {
    pub(super) host: String,
    pub(super) front: String,
    pub(super) window: Window,
    pub(super) webview: WebView,
    pub(super) phase: Phase,
    pub(super) waiting: Vec<String>,
    pub(super) agent: Option<String>,
    pub(super) fetches: PageFetches,
    pub(super) proxy: EventLoopProxy<HelperEvent>,
    pub(super) last_report: Instant,
    /// When the current verify attempt started (for delayed show).
    pub(super) verify_started: Instant,
    pub(super) shown: bool,
}

/// What a window needs to be built.
pub(crate) struct WindowParts<'a> {
    pub(crate) target: &'a EventLoopWindowTarget<HelperEvent>,
    pub(crate) context: &'a mut WebContext,
    pub(crate) proxy: EventLoopProxy<HelperEvent>,
}

impl SiteWindow {
    /// Opens a hidden window on the challenged page (or the front page when that URL is media).
    pub(crate) fn open(
        parts: WindowParts<'_>,
        host: &str,
        challenge_url: &str,
    ) -> Result<Self, String> {
        let front = format!("https://{host}/");
        let page = page_to_open(host, challenge_url);
        let (window, webview) = open_site_view(parts.target, parts.context, host, &page)?;
        tracing::info!(%host, page = %cloudflare_window::short_address(&page), "verification window ready");
        let now = Instant::now();
        Ok(Self {
            host: host.to_owned(),
            front,
            window,
            webview,
            phase: Phase::Verifying(VerifyProgress::starting(now)),
            waiting: Vec::new(),
            agent: None,
            fetches: PageFetches::default(),
            proxy: parts.proxy,
            last_report: now,
            verify_started: now,
            shown: false,
        })
    }

    /// The window's id, to route its events.
    pub(crate) fn window_id(&self) -> tao::window::WindowId {
        self.window.id()
    }

    /// Request `id` waits for this site's check; reloads the challenged page when safe.
    pub(crate) fn begin(&mut self, id: String, challenge_url: &str, out: &Outbox) {
        self.waiting.push(id);
        if matches!(self.phase, Phase::Passed) {
            self.phase = Phase::Verifying(VerifyProgress::starting(Instant::now()));
        }
        let page = page_to_open(&self.host, challenge_url);
        if let Err(error) = self.webview.load_url(&page) {
            tracing::warn!(%error, host = %self.host, "cannot load the verification page");
        }
        self.verify_started = Instant::now();
        self.shown = false;
        out.status(&cloudflare_bridge::HelperStatus::WindowOpen {
            host: self.host.clone(),
        });
    }

    /// Runs the sidecar's request in this window (only once the check passed).
    pub(crate) fn start_fetch(&mut self, request: &FetchRequest, out: &Outbox) {
        if !matches!(self.phase, Phase::Passed) {
            out.fetch_result(
                &request.id,
                &cloudflare_bridge::FetchOutcome::gone("the site is not verified yet"),
            );
            return;
        }
        if let Err(reason) = self.fetches.start(
            &self.webview,
            request,
            crate::page_fetches::PageRequest::Fetch,
        ) {
            out.fetch_result(
                &request.id,
                &cloudflare_bridge::FetchOutcome::failed(reason),
            );
        }
    }

    /// The viewer closed the window: a passed site's window is only hidden (kept for fetches).
    pub(crate) fn close_requested(&mut self, out: &Outbox) {
        match self.phase {
            Phase::Passed => self.window.set_visible(false),
            Phase::Verifying(_) => {
                self.fail(out, "the viewer closed the verification window");
                self.window.set_visible(false);
            }
        }
    }

    /// The app is closing: nobody keeps waiting.
    pub(crate) fn abandon(&mut self, out: &Outbox) {
        let reason = "the app closed".to_owned();
        out.answer_all(
            &std::mem::take(&mut self.waiting),
            &Outcome::Declined(reason.clone()),
        );
        for id in self.fetches.take_all() {
            out.fetch_result(&id, &cloudflare_bridge::FetchOutcome::gone(reason.clone()));
        }
    }
}
