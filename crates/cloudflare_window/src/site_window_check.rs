//! Progress of an open site's Cloudflare check (probe, delayed show, pass/fail).

use std::time::Instant;

use cloudflare_bridge::{FetchOutcome, FetchRequest, HelperStatus, Outcome};
use cloudflare_window::fetch_script::{parse_poll_result, poll_script};
use cloudflare_window::verify_progress::{
    has_clearance, host_of, is_on_site, next_step, page_is_ready, probe_passed, Observation, Step,
    VerifyProgress,
};

use super::{Phase, SiteWindow, REPORT_EVERY, SHOW_AFTER};
use crate::browser_view::read_cookies;
use crate::helper_events::HelperEvent;
use crate::outbox::Outbox;
use crate::page_fetches::PageRequest;

impl SiteWindow {
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
        let ready = page_is_ready(&page);
        let seen = Observation {
            has_clearance: has_clearance(&cookies),
            agent_known: self.agent.is_some(),
            on_site: ready && is_on_site(host_of(&page), &self.host),
            page_ready: ready,
        };
        self.report(host_of(&page), &cookies);
        if self.agent.is_none() {
            let host = self.host.clone();
            self.ask_page("navigator.userAgent", move |raw| HelperEvent::Agent {
                host: host.clone(),
                raw,
            });
        }
        let step = next_step(progress, seen, Instant::now());
        self.maybe_show_for_human(&seen);
        match step {
            Step::Wait => {}
            Step::Probe => self.start_probe(),
            // Off-site after clearance: do not reload the front page (redirect hosts with bad TLS
            // loop forever). Hand over cookies immediately.
            Step::PassOnCookie => self.pass_off_site(out),
            Step::GiveUp => self.fail(out, "the check was not passed in time"),
        }
    }

    /// After the grace period, show any check that has not passed yet — leftover `cf_clearance`
    /// must not hide an unfinished Managed/interactive challenge (`__cf_chl_*` still in the URL).
    fn maybe_show_for_human(&mut self, _seen: &Observation) {
        if self.shown || self.verify_started.elapsed() < SHOW_AFTER {
            return;
        }
        self.window.set_visible(true);
        self.window.set_focus();
        self.shown = true;
        tracing::info!(host = %self.host, "verification still waiting; showing the window");
    }

    fn pass_off_site(&mut self, out: &Outbox) {
        tracing::info!(
            host = %self.host,
            "page left the site after the check; passing with clearance cookies"
        );
        self.pass(out);
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

    pub(super) fn pass(&mut self, out: &Outbox) {
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

    pub(super) fn fail(&mut self, out: &Outbox, reason: &str) {
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

    fn report(&mut self, page_host: &str, cookies: &[(String, String)]) {
        if self.last_report.elapsed() < REPORT_EVERY {
            return;
        }
        self.last_report = Instant::now();
        let names: Vec<&str> = cookies.iter().map(|(name, _)| name.as_str()).collect();
        tracing::info!(host = %self.host, %page_host, ?names, agent_known = self.agent.is_some(), "waiting for the check");
    }

    fn cookies(&self) -> Vec<(String, String)> {
        read_cookies(&self.webview, &self.front)
    }

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
