//! App entry and startup.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use app_services::{PlaybackItem, StreamSelection, UiServices};
use playback_session::{run_open, LiveSources, OpenControl, OpenRequest, RefreshRequest};
use slint::{ComponentHandle, Image, Model, ModelRc, SharedPixelBuffer, VecModel};

use crate::posters::{
    cache_choices, empty_episodes, empty_posters, plugin_rows, posters_from_bookmarks,
    posters_from_continue, posters_from_search, source_rows,
};
use crate::runner::{spawn_work, BrowseRow, EventPump, UiEvent};
use crate::update::{self, UpdateInfo};
use crate::{CloudStream, EpisodeItem, PosterItem};

use super::session::{AppExtras, Session};
use super::wire::wire_callbacks;
use super::events::{drain_cloudflare, drain_events, poll_player};

pub fn run_desktop_app(services: UiServices) -> Result<(), String> {
    run_desktop_app_with(services, AppExtras::default())
}

/// Same as [`run_desktop_app`], with Cloudflare / data-dir handles from the shell.
pub fn run_desktop_app_with(services: UiServices, extras: AppExtras) -> Result<(), String> {
    let ui = CloudStream::new().map_err(|error| error.to_string())?;
    let pump = Rc::new(EventPump::new());
    let session = Rc::new(RefCell::new(Session::default()));
    let services = Rc::new(services);
    let extras = Rc::new(RefCell::new(extras));

    ui.set_home_items(empty_posters());
    ui.set_search_items(empty_posters());
    ui.set_browse_items(empty_posters());
    ui.set_library_items(empty_posters());
    ui.set_continue_items(empty_posters());
    ui.set_episodes(empty_episodes());
    ui.set_sources(ModelRc::new(VecModel::from(Vec::<crate::SourceItem>::new())));
    ui.set_cache_sizes(cache_choices());
    apply_settings_to_ui(&ui, &services);

    wire_callbacks(
        ui.as_weak(),
        Rc::clone(&services),
        Rc::clone(&session),
        Rc::clone(&pump),
        Rc::clone(&extras),
    );
    start_extensions(
        ui.as_weak(),
        Rc::clone(&services),
        pump.sender(),
    );

    let weak = ui.as_weak();
    let services_tick = Rc::clone(&services);
    let session_tick = Rc::clone(&session);
    let pump_tick = Rc::clone(&pump);
    let extras_tick = Rc::clone(&extras);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(200),
        move || {
            if let Some(ui) = weak.upgrade() {
                drain_events(&ui, &services_tick, &session_tick, &pump_tick);
                poll_player(&ui, &services_tick, &session_tick);
                drain_cloudflare(&ui, &extras_tick, &session_tick, &pump_tick);
            }
        },
    );

    ui.run().map_err(|error| error.to_string())?;
    drop(timer);
    Ok(())
}

fn apply_settings_to_ui(ui: &CloudStream, services: &UiServices) {
    ui.set_keep_video_ready(services.settings.keep_video_ready().unwrap_or(true));
    ui.set_autoplay_next(services.settings.autoplay_next().unwrap_or(true));
    ui.set_skip_lookup(services.settings.skip_lookup().unwrap_or(true));
    ui.set_stream_proxy(services.settings.stream_proxy().unwrap_or(true));
    if let Ok(cache) = services.settings.video_cache() {
        ui.set_video_cache_label(format!("{} MB", cache.size_mb).into());
    }
    if let Ok(cap) = services.settings.storage_cap_gb() {
        ui.set_storage_label(format!("Storage limit: {cap} GB").into());
    }
    ui.set_update_label("Updates not checked yet".into());
}

fn start_extensions(
    ui: slint::Weak<CloudStream>,
    services: Rc<UiServices>,
    tx: std::sync::mpsc::Sender<UiEvent>,
) {
    let services = (*services).clone();
    spawn_work("load-extensions", move || {
        let report = services.extensions.load_installed_reporting(&|progress| {
            let _ = tx.send(UiEvent::StartupProgress {
                finished: progress.finished,
                total: progress.total,
                current: progress.current.clone(),
            });
            let _ = ui.upgrade_in_event_loop(|_| {});
        });
        match report {
            Ok(report) => {
                let providers: Vec<String> = report.providers.iter().map(|p| p.name.clone()).collect();
                let error = report.failures.first().map(|(name, reason)| {
                    format!("{name}: {reason}")
                });
                let _ = tx.send(UiEvent::StartupDone { providers, error });
            }
            Err(error) => {
                let _ = tx.send(UiEvent::StartupDone {
                    providers: Vec::new(),
                    error: Some(error.to_string()),
                });
            }
        }
        let _ = ui.upgrade_in_event_loop(|_| {});
    });
}


