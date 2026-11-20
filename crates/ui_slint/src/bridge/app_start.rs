//! Builds the window, wires it, loads the plugins and runs the event loop.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use app_services::UiServices;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::background_work::{spawn_work, EventPump};
use crate::posters::{cache_choices, empty_episodes, empty_posters, empty_shelves};
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::callback_wiring::wire_callbacks;
use super::cloudflare_drain::drain_cloudflare;
use super::debug_logging::apply_saved_debug_logging;
use super::event_dispatch::drain_events;
use super::focus_screens::follow_window_width;
use super::initial_settings::apply_settings_to_ui;
use super::player_polling::{poll_player, show_latest_frame};
use super::session::{AppExtras, Session};

/// How often the UI thread picks up background results and reads the player's status.
const UI_TICK: Duration = Duration::from_millis(200);
/// How often a new video frame is looked for (60 per second).
const FRAME_TICK: Duration = Duration::from_millis(16);

/// Runs the desktop app until its window closes.
pub fn run_desktop_app(services: UiServices) -> Result<(), String> {
    run_desktop_app_with(services, AppExtras::default())
}

/// Same as [`run_desktop_app`], with Cloudflare / data-dir handles from the shell.
pub fn run_desktop_app_with(services: UiServices, extras: AppExtras) -> Result<(), String> {
    let ui = CloudStream::new().map_err(|error| error.to_string())?;
    let ctx = BridgeContext {
        ui: ui.as_weak(),
        services: Rc::new(services),
        session: Rc::new(RefCell::new(Session::default())),
        pump: Rc::new(EventPump::new()),
        extras: Rc::new(RefCell::new(extras)),
    };
    set_empty_models(&ui);
    apply_settings_to_ui(&ui, &ctx.services);
    wire_callbacks(&ctx, &ui);
    apply_saved_debug_logging(&ctx, &ui);
    start_extensions(&ctx);
    let timer = slint::Timer::default();
    let tick_ctx = ctx.clone();
    timer.start(slint::TimerMode::Repeated, UI_TICK, move || {
        if let Some(ui) = tick_ctx.window() {
            drain_events(&tick_ctx, &ui);
            poll_player(&tick_ctx, &ui);
            drain_cloudflare(&tick_ctx, &ui);
            follow_window_width(&ui);
        }
    });
    let frames = slint::Timer::default();
    let frame_ctx = ctx.clone();
    frames.start(slint::TimerMode::Repeated, FRAME_TICK, move || {
        if let Some(ui) = frame_ctx.window() {
            show_latest_frame(&frame_ctx, &ui);
        }
    });
    ui.run().map_err(|error| error.to_string())?;
    ctx.session.borrow().repository_discovery.stop();
    drop(frames);
    drop(timer);
    Ok(())
}

fn set_empty_models(ui: &CloudStream) {
    ui.set_home_shelves(empty_shelves());
    ui.set_search_items(empty_posters());
    ui.set_browse_items(empty_posters());
    ui.set_library_items(empty_posters());
    ui.set_continue_items(empty_posters());
    ui.set_episodes(empty_episodes());
    ui.set_sources(ModelRc::new(
        VecModel::from(Vec::<crate::SourceItem>::new()),
    ));
    ui.set_cache_sizes(cache_choices());
}

fn start_extensions(ctx: &BridgeContext) {
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("load-extensions", move || {
        let report = services.extensions.load_installed_reporting(&|progress| {
            sender.send(UiEvent::StartupProgress {
                finished: progress.finished,
                total: progress.total,
                current: progress.current.clone(),
            });
        });
        let done = match report {
            Ok(report) => UiEvent::StartupDone {
                providers: report.providers.iter().map(|p| p.name.clone()).collect(),
                error: report
                    .failures
                    .first()
                    .map(|(name, reason)| format!("{name}: {reason}")),
            },
            Err(error) => UiEvent::StartupDone {
                providers: Vec::new(),
                error: Some(error.to_string()),
            },
        };
        sender.send(done);
    });
}
