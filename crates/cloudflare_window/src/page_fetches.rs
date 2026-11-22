//! Sidecar requests executed in a verified same-origin document.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cloudflare_bridge::{FetchOutcome, FetchRequest};
use cloudflare_window::fetch_script::{parse_poll_result, poll_script, start_script};
use tao::event_loop::EventLoopProxy;

use crate::browser_view::BrowserView;
use crate::helper_events::HelperEvent;
use crate::outbox::Outbox;

/// How long an in-browser request may take.
const FETCH_TIME: Duration = Duration::from_secs(60);

pub(crate) struct PageFetches {
    pending: HashMap<String, Instant>,
    proxy: EventLoopProxy<HelperEvent>,
    window: String,
}

impl PageFetches {
    pub(crate) fn new(proxy: EventLoopProxy<HelperEvent>, window: String) -> Self {
        Self {
            pending: HashMap::new(),
            proxy,
            window,
        }
    }

    /// Whether a browser fetch is waiting for its answer.
    pub(crate) fn is_busy(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn start(
        &mut self,
        view: &BrowserView,
        request: &FetchRequest,
    ) -> Result<(), String> {
        view.evaluate(&start_script(request), |_| {})?;
        self.pending
            .insert(request.id.clone(), Instant::now() + FETCH_TIME);
        Ok(())
    }

    pub(crate) fn poll(&mut self, view: &BrowserView, out: &Outbox) {
        let now = Instant::now();
        self.pending.retain(|id, deadline| {
            if *deadline <= now {
                out.fetch_result(
                    id,
                    &FetchOutcome::failed("the browser did not answer in time"),
                );
                return false;
            }
            Self::poll_one(view, &self.proxy, &self.window, id);
            true
        });
    }

    fn poll_one(view: &BrowserView, proxy: &EventLoopProxy<HelperEvent>, window: &str, id: &str) {
        let proxy = proxy.clone();
        let script = poll_script(id);
        let window = window.to_owned();
        let request_id = id.to_owned();
        if let Err(error) = view.evaluate(&script, move |raw| {
            if proxy
                .send_event(HelperEvent::Polled {
                    window: window.clone(),
                    id: request_id.clone(),
                    raw,
                })
                .is_err()
            {
                tracing::debug!("the helper closed before a browser fetch answered");
            }
        }) {
            tracing::debug!(%id, %error, "could not poll a browser fetch");
        }
    }

    pub(crate) fn complete(&mut self, id: &str, raw: &str, out: &Outbox) {
        if let Some(result) = parse_poll_result(raw) {
            if self.pending.remove(id).is_some() {
                out.fetch_result(id, &result);
            }
        }
    }

    pub(crate) fn abandon(&mut self, out: &Outbox) {
        for (id, _) in self.pending.drain() {
            out.fetch_result(
                &id,
                &FetchOutcome::gone("the verified document is no longer available"),
            );
        }
    }
}
