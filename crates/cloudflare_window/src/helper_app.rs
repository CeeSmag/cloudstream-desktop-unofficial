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
use crate::helper_events::HelperEvent;
use crate::input_threads::start_input_threads;
use crate::outbox::Outbox;
use crate::site_window::{SiteWindow, WindowParts};

const TICK: Duration = Duration::from_millis(400);

pub(crate) struct HelperConfig {
    pub(crate) exchange: PathBuf,
    pub(crate) profile: PathBuf,
}

struct HelperState {
    sites: HashMap<String, SiteWindow>,
    windows: HashMap<WindowId, String>,
    known: HashSet<String>,
    context: BrowserContext,
    outbox: Outbox,
    proxy: EventLoopProxy<HelperEvent>,
    next_tick: Instant,
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
                if let Some(site) = self.sites.get_mut(&id) {
                    site.close("verification was cancelled", &self.outbox);
                }
                self.retain_context(&id);
            }
            HelperCommand::Quit => {}
        }
    }

    fn show(&mut self, id: &str, activate: bool) {
        if let Some(site) = self.sites.get_mut(id) {
            site.show(activate);
        }
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
            }
            Err(reason) => self.refuse(&request, reason),
        }
    }

    fn fetch(&mut self, request: &FetchRequest) {
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
        }
    }
}
