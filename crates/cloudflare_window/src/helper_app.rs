//! Owns each verification request and the lifetime of its native browser.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cloudflare_bridge::{
    FetchOutcome, FetchRequest, HelperCommand, HelperStatus, Outcome, VerifyRequest,
};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::WindowId;

use crate::browser_view::BrowserContext;
use crate::context_reaper::{contexts_to_release, ContextLimits, ContextState};
use crate::helper_events::HelperEvent;
use crate::input_threads::start_input_threads;
use crate::outbox::Outbox;
use crate::site_window::{SiteWindow, WindowParts};

const TICK: Duration = Duration::from_millis(400);
/// Verified windows the helper keeps when the app does not say (the app always does).
pub(crate) const DEFAULT_MAX_WINDOWS: usize = 4;
/// A verified window nobody has used for this long is released, whatever the limit: a site whose
/// requests go through the browser fetches often, the others need their window only as a fallback.
const CONTEXT_IDLE_TTL: Duration = Duration::from_secs(120);
/// A window used within this time is kept even above the limit, so a burst of sites that each
/// verify at once does not release a window the sidecar is about to use.
const CONTEXT_GRACE: Duration = Duration::from_secs(30);
/// A passed window quiet for this long is moved to the site's small page (`context_slimming.rs`);
/// a fetch that is about to come meets the real page, and a later one the small page.
const SLIM_AFTER: Duration = Duration::from_secs(10);

pub(crate) struct HelperConfig {
    pub(crate) exchange: PathBuf,
    pub(crate) profile: PathBuf,
    /// Verified browser windows kept (each is a browser process).
    pub(crate) max_windows: usize,
}

struct HelperState {
    sites: HashMap<String, SiteWindow>,
    windows: HashMap<WindowId, String>,
    known: HashSet<String>,
    context: BrowserContext,
    outbox: Outbox,
    proxy: EventLoopProxy<HelperEvent>,
    next_tick: Instant,
    limits: ContextLimits,
}

pub(crate) fn run(config: HelperConfig) -> ! {
    let event_loop = EventLoopBuilder::<HelperEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    start_input_threads(&proxy, config.exchange.clone());
    let mut state = HelperState {
        sites: HashMap::new(),
        windows: HashMap::new(),
        known: HashSet::new(),
        context: BrowserContext::new(&config.profile),
        outbox: Outbox {
            exchange: config.exchange,
        },
        proxy,
        next_tick: Instant::now() + TICK,
        limits: ContextLimits::new(config.max_windows, CONTEXT_IDLE_TTL, CONTEXT_GRACE),
    };
    event_loop.run(move |event, target, flow| {
        state.handle(event, target, flow);
        if *flow != ControlFlow::Exit {
            state.tick_if_due();
            *flow = ControlFlow::WaitUntil(state.next_tick);
        }
    })
}

impl HelperState {
    fn handle(
        &mut self,
        event: Event<'_, HelperEvent>,
        target: &EventLoopWindowTarget<HelperEvent>,
        flow: &mut ControlFlow,
    ) {
        match event {
            Event::WindowEvent {
                window_id, event, ..
            } => self.window_event(window_id, event),
            Event::UserEvent(
                HelperEvent::Command(HelperCommand::Quit) | HelperEvent::InputClosed,
            ) => {
                for site in self.sites.values_mut() {
                    site.close("the app closed", &self.outbox);
                    site.fetches.abandon(&self.outbox);
                }
                *flow = ControlFlow::Exit;
            }
            Event::UserEvent(event) => self.user_event(event, target),
            _ => {}
        }
    }

    fn user_event(&mut self, event: HelperEvent, target: &EventLoopWindowTarget<HelperEvent>) {
        match event {
            HelperEvent::Command(command) => self.command(command, target),
            HelperEvent::Fetch(request) => self.fetch(&request),
            HelperEvent::Browser { id } => {
                if let Some(site) = self.sites.get_mut(&id) {
                    site.drain_browser(&self.outbox);
                }
                self.retain_context(&id);
            }
            HelperEvent::Polled { window, id, raw } => {
                if let Some(site) = self.sites.get_mut(&window) {
                    site.fetches.complete(&id, &raw, &self.outbox);
                    site.touch();
                }
            }
            HelperEvent::InputClosed => {}
        }
    }

    fn command(&mut self, command: HelperCommand, target: &EventLoopWindowTarget<HelperEvent>) {
        match command {
            HelperCommand::Verify(request) => self.verify(request, target, false),
            HelperCommand::Present(request) => self.verify(request, target, true),
            HelperCommand::Show { id, .. } => self.show(&id, false),
            HelperCommand::Activate { id, .. } => self.show(&id, true),
            HelperCommand::Cancel { id } => {
                if let Some(key) = self.key_holding(&id) {
                    if let Some(site) = self.sites.get_mut(&key) {
                        site.retire(&id, "verification was cancelled", &self.outbox);
                    }
                    self.retain_context(&key);
                }
            }
            HelperCommand::Quit => {}
        }
    }

    fn show(&mut self, id: &str, activate: bool) {
        if let Some(key) = self.key_holding(id) {
            if let Some(site) = self.sites.get_mut(&key) {
                site.show(activate);
            }
        }
    }

    /// The window that answers request `id`, whether it owns the check or waits with it.
    fn key_holding(&self, id: &str) -> Option<String> {
        self.sites
            .iter()
            .find(|(_, site)| site.holds(id))
            .map(|(key, _)| key.clone())
    }

    fn verify(
        &mut self,
        request: VerifyRequest,
        target: &EventLoopWindowTarget<HelperEvent>,
        present: bool,
    ) {
        if !self.known.insert(request.id.clone()) {
            if present {
                self.show(&request.id, false);
            }
            return;
        }
        if !request.can_navigate() {
            self.refuse(
                &request,
                "this verification request cannot be opened as a browser GET".to_owned(),
            );
            return;
        }
        if let Some(site) = self.sites.values_mut().find(|site| site.joins(&request)) {
            site.join(request.clone());
            if present {
                self.show(&request.id, false);
            }
            return;
        }
        let parts = WindowParts {
            target,
            context: &mut self.context,
            proxy: self.proxy.clone(),
        };
        match SiteWindow::open(parts, request.clone(), present) {
            Ok(site) => {
                site.status_open(&self.outbox);
                self.windows.insert(site.window.id(), request.id.clone());
                self.sites.insert(request.id, site);
                tracing::debug!(windows = self.sites.len(), host = %request.host, "verification window opened");
            }
            Err(reason) => self.refuse(&request, reason),
        }
    }

    fn fetch(&mut self, request: &FetchRequest) {
        // The app never sees this request (the helper answers it), so say so: a helper that is
        // serving browser fetches must not be closed as idle.
        self.outbox.status(&HelperStatus::Fetched {
            host: request.host.clone(),
            id: request.id.clone(),
        });
        if let Some(site) = self.sites.values_mut().find(|site| site.can_fetch(request)) {
            site.start_fetch(request, &self.outbox);
        } else {
            self.outbox.fetch_result(
                &request.id,
                &FetchOutcome::gone("no verified browser document is available for this origin"),
            );
        }
    }

    fn tick_if_due(&mut self) {
        let now = Instant::now();
        if now < self.next_tick {
            return;
        }
        self.next_tick = now + TICK;
        let ids: Vec<_> = self.sites.keys().cloned().collect();
        for id in ids {
            if let Some(site) = self.sites.get_mut(&id) {
                site.tick(&self.outbox);
            }
            self.retain_context(&id);
        }
        self.release_unused_windows();
    }

    /// Releases the verified windows nobody needs (`context_reaper.rs`); a site that needs its
    /// window again is verified again by the sidecar's existing recovery.
    fn release_unused_windows(&mut self) {
        let states: Vec<ContextState<'_>> = self
            .sites
            .iter()
            .map(|(key, site)| ContextState {
                key,
                passed: site.passed(),
                fetching: site.fetching(),
                idle: site.idle(),
            })
            .collect();
        for key in contexts_to_release(&states, &self.limits) {
            tracing::info!(window = %key, "releasing a verified browser window nobody needs");
            self.remove(&key);
        }
        for site in self.sites.values_mut() {
            site.slim_if_quiet(SLIM_AFTER);
        }
    }

    fn refuse(&self, request: &VerifyRequest, reason: String) {
        tracing::warn!(host = %request.host, id = %request.id, %reason, "cannot verify the site");
        let result = self
            .outbox
            .answer(request, &Outcome::Declined(reason.clone()));
        self.outbox.status(&HelperStatus::Failed {
            host: request.host.clone(),
            id: request.id.clone(),
            reason: result.err().unwrap_or(reason),
        });
    }

    fn window_event(&mut self, window_id: WindowId, event: WindowEvent<'_>) {
        let Some(id) = self.windows.get(&window_id).cloned() else {
            return;
        };
        if let Some(site) = self.sites.get_mut(&id) {
            match event {
                WindowEvent::CloseRequested => {
                    site.close("the viewer closed the verification window", &self.outbox)
                }
                WindowEvent::Resized(size) => site.resize(size.width, size.height),
                _ => {}
            }
        }
        self.retain_context(&id);
    }

    fn retain_context(&mut self, id: &str) {
        if self.sites.get(id).is_some_and(SiteWindow::finished) {
            self.remove(id);
        }
        let Some(origin) = self.sites.get(id).and_then(SiteWindow::fetch_origin) else {
            return;
        };
        let replaced: Vec<_> = self
            .sites
            .iter()
            .filter(|(other, site)| other.as_str() != id && site.fetch_origin() == Some(origin))
            .map(|(other, _)| other.clone())
            .collect();
        for other in replaced {
            self.remove(&other);
        }
    }

    fn remove(&mut self, id: &str) {
        if let Some(mut site) = self.sites.remove(id) {
            site.fetches.abandon(&self.outbox);
            self.windows.remove(&site.window.id());
            tracing::debug!(windows = self.sites.len(), "verification window closed");
        }
    }
}
