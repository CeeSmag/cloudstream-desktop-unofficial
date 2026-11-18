//! App entry and startup.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use app_services::UiServices;
use player_api::{all_languages, Language};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::posters::{
    cache_choices, empty_episodes, empty_posters, empty_shelves, string_model,
};
use crate::runner::{spawn_work, EventPump, UiEvent};
use crate::CloudStream;

use super::events::{drain_cloudflare, drain_events, poll_player};
use super::session::{AppExtras, Session};
use super::wire::wire_callbacks;

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
    ui.set_home_shelves(empty_shelves());
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
    start_extensions(ui.as_weak(), Rc::clone(&services), pump.sender());

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
    ui.set_auto_subtitles(services.settings.auto_select_subtitles().unwrap_or(true));

    if let Ok(cache) = services.settings.video_cache() {
        ui.set_video_cache_label(format!("{} MB", cache.size_mb).into());
        ui.set_cache_size_value(format!("{} MB", cache.size_mb).into());
        ui.set_ahead_value(format!("{}%", cache.ahead_percent).into());
    }
    ui.set_cache_size_labels(string_model(
        app_services::VIDEO_CACHE_SIZES_MB
            .iter()
            .map(|mb| format!("{mb} MB")),
    ));
    ui.set_ahead_labels(string_model(
        app_services::VIDEO_CACHE_AHEAD_PERCENTS
            .iter()
            .map(|p| format!("{p}%")),
    ));

    if let Ok(cap) = services.settings.storage_cap_gb() {
        ui.set_storage_label(format!("Storage limit: {cap} GB").into());
        ui.set_storage_value(format!("{cap} GB").into());
    }
    ui.set_storage_labels(string_model((1..=10).map(|gb| format!("{gb} GB"))));

    let worker = services.settings.plugin_worker_limit().unwrap_or(Some(1));
    ui.set_worker_value(worker_label(worker).into());
    ui.set_worker_labels(string_model(
        ["1", "2", "Unlimited"].into_iter().map(str::to_owned),
    ));

    let audio_labels = language_labels();
    ui.set_audio_labels(string_model(audio_labels.clone()));
    ui.set_subtitle_labels(string_model(audio_labels));
    match services.settings.preferred_audio() {
        Ok(Some(lang)) => ui.set_audio_value(lang.name.into()),
        Ok(None) => ui.set_audio_value("Stream default".into()),
        Err(_) => ui.set_audio_value("English".into()),
    }
    if let Ok(subs) = services.settings.preferred_subtitles() {
        let chips = subs
            .iter()
            .map(|l| l.name)
            .collect::<Vec<_>>()
            .join(", ");
        ui.set_subtitle_chips(chips.into());
    }
    ui.set_subtitle_add_value("English".into());
    ui.set_update_label("Updates not checked yet".into());
}

fn worker_label(limit: Option<usize>) -> String {
    match limit {
        Some(1) => "1".into(),
        Some(2) => "2".into(),
        _ => "Unlimited".into(),
    }
}

fn language_labels() -> Vec<String> {
    let mut labels = vec!["Stream default".to_owned()];
    labels.extend(all_languages().iter().map(|l: &Language| l.name.to_owned()));
    labels
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
                let providers: Vec<String> =
                    report.providers.iter().map(|p| p.name.clone()).collect();
                let error = report
                    .failures
                    .first()
                    .map(|(name, reason)| format!("{name}: {reason}"));
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
