//! One site's verification window: the check, then browser fetches for the sidecar.

use std::time::{Duration, Instant};

use cloudflare_bridge::{FetchOutcome, FetchRequest, HelperStatus, Outcome};
use cloudflare_window::fetch_script::{parse_poll_result, poll_script};
use cloudflare_window::verify_progress::{
    has_clearance, host_of, is_on_site, next_step, probe_passed, Observation, Step, VerifyProgress,
};
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::window::Window;
use wry::{WebContext, WebView};

use crate::browser_view::{open_site_view, read_cookies};
use crate::helper_events::HelperEvent;
use crate::outbox::Outbox;
use crate::page_fetches::{PageFetches, PageRequest};

/// How often a waiting window says what it sees.
const REPORT_EVERY: Duration = Duration::from_secs(5);

enum Phase {
    Verifying(VerifyProgress),
    Passed,
}

/// A browser window on one site.
pub(crate) struct SiteWindow {
    host: String,
    front: String,
    window: Window,
    webview: WebView,
    phase: Phase,
    waiting: Vec<String>,
    agent: Option<String>,
    fetches: PageFetches,
    proxy: EventLoopProxy<HelperEvent>,
    last_report: Instant,
}

/// What a window needs to be built.
pub(crate) struct WindowParts<'a> {
    pub(crate) target: &'a EventLoopWindowTarget<HelperEvent>,
    pub(crate) context: &'a mut WebContext,
    pub(crate) proxy: EventLoopProxy<HelperEvent>,
}

impl SiteWindow {
    /// Opens a window on `host`'s front page (never the request's own address, which can be a
    /// video or an API call that a browser would download). Downloads are refused.
    pub(crate) fn open(parts: WindowParts<'_>, host: &str) -> Result<Self, String> {
        let front = format!("https://{host}/");
        let (window, webview) = open_site_view(parts.target, parts.context, host)?;
        tracing::info!(%host, "verification window open");
        Ok(Self {
            host: host.to_owned(),
            front,
            window,
            webview,
            phase: Phase::Verifying(VerifyProgress::starting(Instant::now())),
            waiting: Vec::new(),
            agent: None,
            fetches: PageFetches::default(),
            proxy: parts.proxy,
            last_report: Instant::now(),
        })
    }

    /// The window's id, to route its events.
    pub(crate) fn window_id(&self) -> tao::window::WindowId {
        self.window.id()
    }

    /// Request `id` waits for this site's check; a passed site is checked again (its clearance
    /// stopped working for the sidecar), which passes at once while the browser's own still works.
    pub(crate) fn begin(&mut self, id: String, out: &Outbox) {
        self.waiting.push(id);
        if matches!(self.phase, Phase::Passed) {
            self.phase = Phase::Verifying(VerifyProgress::starting(Instant::now()));
        }
        self.window.set_visible(true);
        self.window.set_focus();
        out.status(&HelperStatus::WindowOpen {
            host: self.host.clone(),
        });
    }

    /// Looks at the window once: running fetches, then the check's progress.
    pub(crate) fn tick(&mut self, out: &Outbox) {
        self.expire_fetches(out);
        for id in self.fetches.ids() {
            self.ask_page(&poll_script(&id), move |raw| HelperEvent::Polled {
                id: id.clone(),
                raw,
            });
        }
        if let Phase::Verifying(progress) = &self.phase {
            let progress = progress.clone();
            self.look_at_check(&progress, out);
        }
    }

    fn look_at_check(&mut self, progress: &VerifyProgress, out: &Outbox) {
        let cookies = self.cookies();
        let page = self.webview.url().unwrap_or_default();
        let seen = Observation {
            has_clearance: has_clearance(&cookies),
            agent_known: self.agent.is_some(),
            on_site: is_on_site(host_of(&page), &self.host),
        };
        self.report(host_of(&page), &cookies);
        if self.agent.is_none() {
            let host = self.host.clone();
            self.ask_page("navigator.userAgent", move |raw| HelperEvent::Agent {
                host: host.clone(),
                raw,
            });
        }
        match next_step(progress, seen, Instant::now()) {
            Step::Wait => {}
            Step::Probe => self.start_probe(),
            Step::PassOnCookie => {
                tracing::info!(host = %self.host, "the page left the site after the check; passing on its clearance cookie");
                self.pass(out);
            }
            Step::GiveUp => self.fail(out, "the check was not passed in time"),
        }
    }

    /// The browser's `User-Agent` arrived (JSON-encoded on some engines, plain on WebKitGTK).
    pub(crate) fn set_agent(&mut self, raw: &str) {
        let agent = cloudflare_window::script_text(raw);
        if !agent.is_empty() {
            self.agent = Some(agent);
        }
    }

    /// A fetch poll answered; finishes the fetch or the probe when it is done.
    pub(crate) fn on_polled(&mut self, id: &str, raw: &str, out: &Outbox) {
        let Some(outcome) = parse_poll_result(raw) else {
            return;
        };
        let Some(job) = self.fetches.take(id) else {
            return;
        };
        if job.kind == PageRequest::Fetch {
            out.fetch_result(id, &outcome);
            return;
        }
        if let Phase::Verifying(progress) = &mut self.phase {
            progress.probing = false;
        }
        if let FetchOutcome::Done {
            status, headers, ..
        } = &outcome
        {
            if probe_passed(*status, headers) {
                self.pass(out);
            }
        }
    }

    /// Runs the sidecar's request in this window (only once the check passed).
    pub(crate) fn start_fetch(&mut self, request: &FetchRequest, out: &Outbox) {
        if !matches!(self.phase, Phase::Passed) {
            out.fetch_result(
                &request.id,
                &FetchOutcome::gone("the site is not verified yet"),
            );
            return;
        }
        if let Err(reason) = self
            .fetches
            .start(&self.webview, request, PageRequest::Fetch)
        {
            out.fetch_result(&request.id, &FetchOutcome::failed(reason));
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
            out.fetch_result(&id, &FetchOutcome::gone(reason.clone()));
        }
    }

    fn start_probe(&mut self) {
        let probe = FetchRequest {
            id: self.fetches.next_probe_id(),
            host: self.host.clone(),
            method: "GET".to_owned(),
            url: self.front.clone(),
            referer: None,
            headers: Vec::new(),
            body_base64: None,
        };
        let started = self
            .fetches
            .start(&self.webview, &probe, PageRequest::Probe);
        if let Phase::Verifying(progress) = &mut self.phase {
            progress.probing = started.is_ok();
        }
        if let Err(reason) = started {
            tracing::debug!(%reason, host = %self.host, "the probe did not start; trying again");
        }
    }

    fn expire_fetches(&mut self, out: &Outbox) {
        for (id, job) in self.fetches.take_expired(Instant::now()) {
            if job.kind == PageRequest::Probe {
                if let Phase::Verifying(progress) = &mut self.phase {
                    progress.probing = false;
                }
            } else {
                let outcome = FetchOutcome::failed("the browser did not answer in time");
                out.fetch_result(&id, &outcome);
            }
        }
    }

    fn pass(&mut self, out: &Outbox) {
        let passed = Outcome::Passed {
            user_agent: self.agent.clone().unwrap_or_default(),
            cookies: self.cookies(),
        };
        out.answer_all(&std::mem::take(&mut self.waiting), &passed);
        self.phase = Phase::Passed;
        self.window.set_visible(false);
        tracing::info!(host = %self.host, "the check passed; the window is hidden and kept for browser fetches");
        out.status(&HelperStatus::Passed {
            host: self.host.clone(),
        });
    }

    fn fail(&mut self, out: &Outbox, reason: &str) {
        out.answer_all(
            &std::mem::take(&mut self.waiting),
            &Outcome::Declined(reason.to_owned()),
        );
        self.phase = Phase::Verifying(VerifyProgress::starting(Instant::now()));
        tracing::info!(host = %self.host, %reason, "the check was not passed");
        out.status(&HelperStatus::Failed {
            host: self.host.clone(),
            reason: reason.to_owned(),
        });
    }

    /// Every few seconds: what the window shows (cookie names only), so a stuck check can be
    /// told from a slow one in the log.
    fn report(&mut self, page_host: &str, cookies: &[(String, String)]) {
        if self.last_report.elapsed() < REPORT_EVERY {
            return;
        }
        self.last_report = Instant::now();
        let names: Vec<&str> = cookies.iter().map(|(name, _)| name.as_str()).collect();
        tracing::info!(host = %self.host, %page_host, ?names, agent_known = self.agent.is_some(), "waiting for the check");
    }

    /// The window's cookies for the site.
    fn cookies(&self) -> Vec<(String, String)> {
        read_cookies(&self.webview, &self.front)
    }

    /// Runs `script` and hands its JSON answer to the event loop as `event(raw)`.
    fn ask_page(&self, script: &str, event: impl Fn(String) -> HelperEvent + Send + 'static) {
        let proxy = self.proxy.clone();
        let sent = self
            .webview
            .evaluate_script_with_callback(script, move |raw| {
                if proxy.send_event(event(raw)).is_err() {
                    tracing::debug!("the helper is closing; a page answer is dropped");
                }
            });
        if let Err(error) = sent {
            tracing::debug!(%error, host = %self.host, "the page did not take a script");
        }
    }
}
