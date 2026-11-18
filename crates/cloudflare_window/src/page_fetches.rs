//! Requests running inside a site's page: the sidecar's browser fetches and the check's probes.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cloudflare_bridge::FetchRequest;
use cloudflare_window::fetch_script::start_script;
use wry::WebView;

/// How long one browser fetch may take.
const FETCH_TIME: Duration = Duration::from_secs(60);
/// How long the probe of a passed check may take.
const PROBE_TIME: Duration = Duration::from_secs(10);

/// Who a request in the page is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageRequest {
    /// The sidecar's browser fetch: its answer goes to the sidecar.
    Fetch,
    /// A probe of the check: its answer decides whether the check passed.
    Probe,
}

/// One request running in the page.
pub(crate) struct FetchJob {
    deadline: Instant,
    /// Who the answer is for.
    pub(crate) kind: PageRequest,
}

/// The requests running in one page.
#[derive(Default)]
pub(crate) struct PageFetches {
    jobs: HashMap<String, FetchJob>,
    probes: u32,
}

impl PageFetches {
    /// Starts `request` in the page; its answer is read back by polling [`Self::ids`].
    pub(crate) fn start(
        &mut self,
        webview: &WebView,
        request: &FetchRequest,
        kind: PageRequest,
    ) -> Result<(), String> {
        webview
            .evaluate_script(&start_script(request))
            .map_err(|error| format!("cannot run the request in the window: {error}"))?;
        let limit = match kind {
            PageRequest::Fetch => FETCH_TIME,
            PageRequest::Probe => PROBE_TIME,
        };
        let job = FetchJob {
            deadline: Instant::now() + limit,
            kind,
        };
        self.jobs.insert(request.id.clone(), job);
        Ok(())
    }

    /// A fresh id for a probe request (letters, digits and `-`, safe in a script).
    pub(crate) fn next_probe_id(&mut self) -> String {
        self.probes += 1;
        format!("probe-{}", self.probes)
    }

    /// The ids of every running request.
    pub(crate) fn ids(&self) -> Vec<String> {
        self.jobs.keys().cloned().collect()
    }

    /// Takes request `id` out (its answer arrived).
    pub(crate) fn take(&mut self, id: &str) -> Option<FetchJob> {
        self.jobs.remove(id)
    }

    /// Takes out every request whose time ran out at `now`.
    pub(crate) fn take_expired(&mut self, now: Instant) -> Vec<(String, FetchJob)> {
        let expired: Vec<String> = self
            .jobs
            .iter()
            .filter(|(_, job)| job.deadline <= now)
            .map(|(id, _)| id.clone())
            .collect();
        expired
            .into_iter()
            .filter_map(|id| self.jobs.remove(&id).map(|job| (id, job)))
            .collect()
    }

    /// Takes out every request (the app is closing).
    pub(crate) fn take_all(&mut self) -> Vec<String> {
        std::mem::take(&mut self.jobs).into_keys().collect()
    }
}
