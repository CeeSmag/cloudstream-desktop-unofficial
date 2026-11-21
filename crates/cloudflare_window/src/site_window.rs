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
pub(super) enum Phase {
    Idle,
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
        })
    }

    /// The window's id, to route its events.
    pub(crate) fn window_id(&self) -> tao::window::WindowId {
        self.window.id()
    }

    /// Request `id` waits for this site's check; reloads the challenged page when safe.
    pub(crate) fn begin(&mut self, id: String, challenge_url: &str, out: &Outbox) {
        if self.waiting.contains(&id) {
            return;
        }
        self.waiting.push(id.clone());
        if !matches!(self.phase, Phase::Verifying(_)) {
            self.phase = Phase::Verifying(VerifyProgress::starting(Instant::now()));
            let page = page_to_open(&self.host, challenge_url);
            if let Err(error) = self.webview.load_url(&page) {
                tracing::warn!(%error, host = %self.host, "cannot load the verification page");
            }
        }
        out.status(&cloudflare_bridge::HelperStatus::WindowOpen {
            id,
            host: self.host.clone(),
        });
    }

    fn hide(&self) {
        #[cfg(windows)]
        crate::windows_presentation::hide(&self.window);
        #[cfg(not(windows))]
        self.window.set_visible(false);
    }

    /// Presents only a check that is still active.
    pub(crate) fn present(&self, id: &str) {
        if self.waiting.iter().any(|pending| pending == id) {
            #[cfg(windows)]
            crate::windows_presentation::show(&self.window);
            #[cfg(not(windows))]
            {
                self.window.set_visible(true);
                self.window.set_focus();
            }
            tracing::debug!(host = %self.host, "viewer opened verification");
        }
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
            Phase::Idle | Phase::Passed => self.hide(),
            Phase::Verifying(_) => {
                self.fail(out, "the viewer closed the verification window");
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
