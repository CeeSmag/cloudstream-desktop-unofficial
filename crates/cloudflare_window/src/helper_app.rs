//! The helper's event loop: one window per site, looked at a few times a second.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cloudflare_bridge::{FetchOutcome, HelperCommand, HelperStatus, Outcome, VerifyRequest};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::WindowId;
use wry::WebContext;

use crate::helper_events::HelperEvent;
use crate::input_threads::start_input_threads;
use crate::outbox::Outbox;
use crate::site_window::{SiteWindow, WindowParts};

/// How often every window is looked at.
const TICK: Duration = Duration::from_millis(400);

/// Where the helper keeps things.
pub(crate) struct HelperConfig {
    /// The sidecar's exchange folder.
    pub(crate) exchange: PathBuf,
    /// The browser profile (cookies survive restarts, so a passed site stays passed).
    pub(crate) profile: PathBuf,
}

struct HelperState {
    sites: HashMap<String, SiteWindow>,
    windows: HashMap<WindowId, String>,
    context: WebContext,
    outbox: Outbox,
    proxy: EventLoopProxy<HelperEvent>,
    /// When the windows are looked at next. Fixed, not "TICK after the last event": the browser
    /// keeps sending events, which would otherwise push the tick away for ever.
    next_tick: Instant,
}

/// Runs until the app closes the helper's stdin.
pub(crate) fn run(config: HelperConfig) -> ! {
    let event_loop = EventLoopBuilder::<HelperEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    start_input_threads(&proxy, config.exchange.clone());
    let mut state = HelperState {
        sites: HashMap::new(),
        windows: HashMap::new(),
        context: WebContext::new(Some(config.profile)),
        outbox: Outbox {
            exchange: config.exchange,
        },
        proxy,
        next_tick: Instant::now() + TICK,
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
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => self.close_requested(window_id),
            Event::UserEvent(event) => self.user_event(event, target, flow),
            _ => {}
        }
    }

    fn user_event(
        &mut self,
        event: HelperEvent,
        target: &EventLoopWindowTarget<HelperEvent>,
        flow: &mut ControlFlow,
    ) {
        match event {
            HelperEvent::Command(HelperCommand::Verify(request)) => {
                self.verify(request, target, false)
            }
            HelperEvent::Command(HelperCommand::Present(request)) => {
                self.verify(request, target, true)
            }
            HelperEvent::Command(HelperCommand::Show { host, id }) => {
                if let Some(site) = self.sites.get(&host) {
                    site.present(&id);
                }
            }
            HelperEvent::Command(HelperCommand::Activate { host, id }) => {
                self.activate(&host, &id);
            }
            HelperEvent::Command(HelperCommand::Quit) | HelperEvent::InputClosed => {
                for site in self.sites.values_mut() {
                    site.abandon(&self.outbox);
                }
                *flow = ControlFlow::Exit;
            }
            HelperEvent::Fetch(request) => match self.sites.get_mut(&request.host) {
                Some(site) => site.start_fetch(&request, &self.outbox),
                None => self.outbox.fetch_result(
                    &request.id,
                    &FetchOutcome::gone("no verification window is open for this site"),
                ),
            },
            HelperEvent::Agent { host, raw } => {
                if let Some(site) = self.sites.get_mut(&host) {
                    site.set_agent(&raw);
                }
            }
            HelperEvent::Polled { id, raw } => {
                for site in self.sites.values_mut() {
                    site.on_polled(&id, &raw, &self.outbox);
                }
            }
        }
    }

    fn activate(&self, host: &str, id: &str) {
        #[cfg(windows)]
        if let Some(site) = self.sites.get(host) {
            if site.waiting.iter().any(|pending| pending == id) {
                crate::windows_presentation::activate(&site.window);
            }
        }
        #[cfg(not(windows))]
        tracing::debug!(%host, %id, "ignoring Windows activation command");
    }

    fn verify(
        &mut self,
        request: VerifyRequest,
        target: &EventLoopWindowTarget<HelperEvent>,
        present: bool,
    ) {
        let reuse = self.sites.contains_key(&request.host);
        if !reuse {
            let parts = WindowParts {
                target,
                context: &mut self.context,
                proxy: self.proxy.clone(),
            };
            match SiteWindow::open(parts, &request.host, &request.url) {
                Ok(site) => {
                    self.windows.insert(site.window_id(), request.host.clone());
                    self.sites.insert(request.host.clone(), site);
                }
                Err(reason) => return self.refuse(&request, reason),
            }
        }
        if let Some(site) = self.sites.get_mut(&request.host) {
            site.begin(request.id.clone(), &request.url, &self.outbox);
            if present {
                site.present(&request.id);
            }
        }
    }

    fn tick_if_due(&mut self) {
        let now = Instant::now();
        if now < self.next_tick {
            return;
        }
        self.next_tick = now + TICK;
        for site in self.sites.values_mut() {
            site.tick(&self.outbox);
        }
    }

    fn refuse(&self, request: &VerifyRequest, reason: String) {
        tracing::warn!(host = %request.host, %reason, "cannot verify the site");
        self.outbox.answer_all(
            std::slice::from_ref(&request.id),
            &Outcome::Declined(reason.clone()),
        );
        self.outbox.status(&HelperStatus::Failed {
            host: request.host.clone(),
            reason,
        });
    }

    fn close_requested(&mut self, window_id: WindowId) {
        let site = self
            .windows
            .get(&window_id)
            .and_then(|host| self.sites.get_mut(host));
        if let Some(site) = site {
            site.close_requested(&self.outbox);
        }
    }
}
