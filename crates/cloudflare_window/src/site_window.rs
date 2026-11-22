//! A verification window owns one request and at most one verified browser context.

use std::rc::Rc;
use std::time::{Duration, Instant};

use cloudflare_bridge::{FetchOutcome, FetchRequest, HelperStatus, Outcome, VerifyRequest};
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::Window;
use url::{Origin, Url};

use crate::browser_view::{open_view, BrowserContext, BrowserSource, BrowserView, RecycleParts};
use crate::context_slimming::{should_slim, slim_address, SlimState};
use crate::helper_events::HelperEvent;
use crate::outbox::Outbox;
use crate::page_fetches::PageFetches;
use crate::verification::{
    BrowserEvent, BrowserIdentity, Cookies, Decision, DocumentId, MainResponse, ObservedResponse,
    Verification,
};
use crate::waiters::{Retired, Waiters};

const VIEWER_TIME: Duration = Duration::from_secs(4 * 60);
const UNCONFIRMED: &str = "the browser could not confirm clearance for this request";

enum Phase {
    Loading,
    Checking(Verification),
    Passed,
    Finished,
}

pub(crate) struct SiteWindow {
    view: BrowserView,
    browser_events: BrowserSource,
    pub(crate) window: Rc<Window>,
    waiters: Waiters,
    target: Url,
    phase: Phase,
    deadline: Instant,
    agent: String,
    requested: bool,
    shown: bool,
    foreground: bool,
    committed: Option<(DocumentId, Origin)>,
    pub(crate) fetches: PageFetches,
    /// When the window last did something for the sidecar (passed, or a browser fetch).
    last_used: Instant,
    /// Moved to the site's small page (`context_slimming.rs`) to free the heavy page's memory.
    slimmed: bool,
}

pub(crate) struct WindowParts<'a> {
    pub(crate) target: &'a EventLoopWindowTarget<HelperEvent>,
    pub(crate) context: &'a mut BrowserContext,
    pub(crate) proxy: EventLoopProxy<HelperEvent>,
}

impl SiteWindow {
    pub(crate) fn open(
        parts: WindowParts<'_>,
        request: VerifyRequest,
        present: bool,
    ) -> Result<Self, String> {
        let target = Url::parse(&request.url).map_err(|_| "the challenge address is invalid")?;
        let source = BrowserSource::new(target.clone(), request.id.clone(), parts.proxy.clone());
        let (view, window) = open_view(parts.target, parts.context, &request.host, source.clone())?;
        let fetches = PageFetches::new(parts.proxy, request.id.clone());
        Ok(Self {
            view,
            browser_events: source,
            window,
            waiters: Waiters::new(request),
            target,
            phase: Phase::Loading,
            deadline: Instant::now() + VIEWER_TIME,
            agent: String::new(),
            requested: present,
            shown: false,
            foreground: false,
            committed: None,
            fetches,
            last_used: Instant::now(),
            slimmed: false,
        })
    }

    pub(crate) fn status_open(&self, out: &Outbox) {
        out.status(&HelperStatus::WindowOpen {
            host: self.waiters.owner().host.clone(),
            id: self.waiters.owner().id.clone(),
        });
    }

    pub(crate) fn resize(&self, width: u32, height: u32) {
        self.view.resize(width, height);
    }

    pub(crate) fn show(&mut self, activate: bool) {
        if self.is_pending() {
            self.requested = true;
            self.foreground |= activate;
            self.reveal();
        }
    }

    fn reveal(&mut self) {
        let Phase::Checking(verification) = &self.phase else {
            return;
        };
        let Some(document) = verification.visible_document().filter(|_| self.requested) else {
            return;
        };
        if !self.view.reveal(document) {
            return;
        }
        if !self.shown {
            self.shown = true;
            #[cfg(windows)]
            crate::windows_presentation::show(&self.window);
            #[cfg(not(windows))]
            {
                self.window.set_visible(true);
                self.window.set_focus();
            }
        }
        if std::mem::take(&mut self.foreground) {
            #[cfg(windows)]
            crate::windows_presentation::activate(&self.window);
        }
    }

    pub(crate) fn drain_browser(&mut self, out: &Outbox) {
        while let Some(event) = self.browser_events.take_event() {
            self.browser_event(event, out);
        }
    }

    fn browser_event(&mut self, event: BrowserEvent, out: &Outbox) {
        match event {
            BrowserEvent::Ready if matches!(self.phase, Phase::Loading) => self.view.identify(),
            BrowserEvent::Identity(result) if matches!(self.phase, Phase::Loading) => {
                self.identified(result, out)
            }
            BrowserEvent::Started(document) => {
                self.committed = None;
                self.fetches.abandon(out);
                if let Phase::Checking(check) = &mut self.phase {
                    check.start(document);
                }
            }
            BrowserEvent::Committed(document, url) => {
                self.committed = Some((document, url.origin()));
                if matches!(self.phase, Phase::Passed) && url.origin() != self.target.origin() {
                    self.phase = Phase::Finished;
                }
                if let Phase::Checking(check) = &mut self.phase {
                    check.commit(document);
                }
                self.reveal();
            }
            BrowserEvent::Response(response) => self.response(response, out),
            BrowserEvent::Unavailable(reason) => self.fail_pending(&reason, out),
            _ => {}
        }
    }

    fn response(&mut self, observed: ObservedResponse, out: &Outbox) {
        let ObservedResponse {
            document,
            response,
            cookies,
        } = observed;
        let leaves_origin = response.leaves_origin(&self.target);
        if matches!(self.phase, Phase::Passed) && leaves_origin {
            self.phase = Phase::Finished;
        }
        if self.slimmed && matches!(self.phase, Phase::Passed) && response.challenge {
            // The small page was challenged: this document cannot serve fetches. Drop the window;
            // the sidecar verifies the site again when it needs it.
            tracing::debug!(id = %self.waiters.owner().id, "the small page was challenged; dropping the window");
            self.phase = Phase::Finished;
        }
        if !matches!(self.phase, Phase::Checking(_)) {
            return;
        }
        self.log_response(document, &response, &cookies);
        let Phase::Checking(check) = &mut self.phase else {
            return;
        };
        match check.response(document, &response, cookies) {
            Decision::Passed(cookies) => {
                let outcome = Outcome::Passed {
                    user_agent: self.agent.clone(),
                    cookies,
                };
                self.complete(outcome, out);
                if leaves_origin || response.attachment {
                    self.phase = Phase::Finished;
                }
            }
            Decision::BrowserClear => {
                let user_agent = self.agent.clone();
                self.complete(Outcome::BrowserClear { user_agent }, out);
            }
            Decision::Unconfirmed => self.fail_pending(UNCONFIRMED, out),
            Decision::Wait => self.reveal(),
        }
    }

    /// What the browser's main response says: where a redirect leads and which cookies exist (names only).
    fn log_response(&self, document: DocumentId, response: &MainResponse, cookies: &Cookies) {
        let names: Vec<&str> = cookies.iter().map(|(name, _)| name.as_str()).collect();
        let redirect_origin = response
            .redirect
            .as_ref()
            .map(|url| url.origin().ascii_serialization());
        let leaves_origin = response.leaves_origin(&self.target);
        tracing::debug!(
            id = %self.waiters.owner().id, ?document, status = response.status, challenged = response.challenge,
            leaves_origin, ?redirect_origin, cookie_names = ?names, "verification main response"
        );
    }

    fn identified(&mut self, result: Result<BrowserIdentity, String>, out: &Outbox) {
        match result {
            Ok(identity) => {
                self.agent = identity.agent;
                self.phase =
                    Phase::Checking(Verification::new(self.target.clone(), identity.cookies));
                self.view.navigate(&self.target);
            }
            Err(reason) => self.fail_pending(&reason, out),
        }
    }

    pub(crate) fn can_fetch(&self, request: &FetchRequest) -> bool {
        self.fetch_origin()
            .is_some_and(|origin| Url::parse(&request.url).is_ok_and(|url| &url.origin() == origin))
    }

    pub(crate) fn fetch_origin(&self) -> Option<&Origin> {
        self.committed.as_ref().and_then(|(document, origin)| {
            (matches!(self.phase, Phase::Passed)
                && origin == &self.target.origin()
                && self.view.is_current(*document))
            .then_some(origin)
        })
    }

    pub(crate) fn start_fetch(&mut self, request: &FetchRequest, out: &Outbox) {
        self.touch();
        if let Err(reason) = self.fetches.start(&self.view, request) {
            out.fetch_result(&request.id, &FetchOutcome::failed(reason));
        }
    }

    pub(crate) fn tick(&mut self, out: &Outbox) {
        self.fetches.poll(&self.view, out);
        if self.is_pending() && Instant::now() >= self.deadline {
            self.close("the check was not passed in time", out);
        }
    }

    pub(crate) fn close(&mut self, reason: &str, out: &Outbox) {
        // The native callback may have recorded a pass before the OS close message
        // even when the event-loop wakeup for it is still queued.
        self.drain_browser(out);
        self.fail_pending(reason, out);
    }

    fn fail_pending(&mut self, reason: &str, out: &Outbox) {
        if self.is_pending() {
            self.complete(Outcome::Declined(reason.to_owned()), out);
        }
    }

    /// The check passed and the window is kept for the sidecar's browser fetches.
    pub(crate) fn passed(&self) -> bool {
        matches!(self.phase, Phase::Passed)
    }

    /// A browser fetch is running in this window.
    pub(crate) fn fetching(&self) -> bool {
        self.fetches.is_busy()
    }

    /// Time since the window last did something for the sidecar.
    pub(crate) fn idle(&self) -> Duration {
        self.last_used.elapsed()
    }

    /// Moves a passed window that has been quiet for `after` to the site's small page, so the
    /// heavy page's memory is freed while the window can still serve browser fetches.
    pub(crate) fn slim_if_quiet(&mut self, after: Duration, context: &mut BrowserContext) {
        let state = SlimState {
            passed: self.passed(),
            slimmed: self.slimmed,
            fetching: self.fetching(),
            idle: self.idle(),
        };
        if !should_slim(&state, after) {
            return;
        }
        self.slimmed = true;
        if let Some(address) = slim_address(&self.target) {
            tracing::debug!(id = %self.waiters.owner().id, %address, "moving a quiet verified window to a small page");
            let parts = RecycleParts {
                context,
                window: &self.window,
                source: &self.browser_events,
            };
            self.view.recycle(parts, &address);
        }
    }

    /// Notes that the window was just used.
    pub(crate) fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    pub(crate) fn finished(&self) -> bool {
        matches!(self.phase, Phase::Finished)
    }

    fn is_pending(&self) -> bool {
        matches!(self.phase, Phase::Loading | Phase::Checking(_))
    }

    fn complete(&mut self, outcome: Outcome, out: &Outbox) {
        self.phase = Phase::Finished;
        self.requested = false;
        self.foreground = false;
        #[cfg(windows)]
        crate::windows_presentation::hide(&self.window);
        #[cfg(not(windows))]
        self.window.set_visible(false);
        for (index, request) in self.waiters.take_all().into_iter().enumerate() {
            let status = out.deliver(&request, &outcome);
            if index == 0 && matches!(status, HelperStatus::Passed { .. }) {
                self.phase = Phase::Passed;
                self.touch();
            }
            tracing::debug!(id = %request.id, host = %request.host, ?status, "verification finished");
            out.status(&status);
        }
    }

    pub(crate) fn joins(&self, request: &VerifyRequest) -> bool {
        self.is_pending() && self.waiters.joins(request)
    }

    /// `request` waits for this check instead of opening a window of its own.
    pub(crate) fn join(&mut self, request: VerifyRequest) {
        tracing::debug!(id = %request.id, owner = %self.waiters.owner().id, host = %request.host, "request joins the pending check");
        self.waiters.join(request);
    }

    pub(crate) fn holds(&self, id: &str) -> bool {
        self.waiters.holds(id)
    }

    /// Retires one request with `reason`; the check goes on for the others.
    pub(crate) fn retire(&mut self, id: &str, reason: &str, out: &Outbox) {
        if !self.is_pending() {
            return;
        }
        match self.waiters.retire(id) {
            Retired::Last(_) => self.fail_pending(reason, out),
            Retired::Owner(request) | Retired::Waiter(request) => {
                out.status(&out.deliver(&request, &Outcome::Declined(reason.to_owned())));
            }
            Retired::Unknown => {}
        }
    }
}
