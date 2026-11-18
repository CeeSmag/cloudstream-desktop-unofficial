//! Settings callbacks.
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
use super::catalog::load_home;
use super::settings_extra::{
    pump_dummy, refresh_plugins, tx_notice, tx_notice_from_extras, wire_cloudflare,
};


pub(crate) fn wire_settings(
    some_ui: &CloudStream,
    ui: slint::Weak<CloudStream>,
    services: Rc<UiServices>,
    session: Rc<RefCell<Session>>,
    pump: Rc<EventPump>,
    extras: Rc<RefCell<AppExtras>>,
) {
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_keep_ready(move |on| {
            let _ = services.settings.set_keep_video_ready(on);
            if let Some(ui) = ui.upgrade() {
                ui.set_keep_video_ready(on);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_autoplay(move |on| {
            let _ = services.settings.set_autoplay_next(on);
            if let Some(ui) = ui.upgrade() {
                ui.set_autoplay_next(on);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_skip_lookup(move |on| {
            let _ = services.settings.set_skip_lookup(on);
            if let Some(ui) = ui.upgrade() {
                ui.set_skip_lookup(on);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_stream_proxy(move |on| {
            let _ = services.settings.set_stream_proxy(on);
            if let Some(ui) = ui.upgrade() {
                ui.set_stream_proxy(on);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_video_cache(move |value| {
            if let Ok(mb) = value.parse::<u32>() {
                if let Ok(mut cache) = services.settings.video_cache() {
                    cache.size_mb = mb;
                    let _ = services.settings.set_video_cache(cache);
                    if let Some(ui) = ui.upgrade() {
                        ui.set_video_cache_label(format!("{mb} MB").into());
                    }
                    if let Some(playback) = &services.playback {
                        let _ = playback.set_cache_limits(cache.while_watching());
                    }
                }
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_provider(move |name| {
            if let Some(ui) = ui.upgrade() {
                ui.set_selected_provider(name.clone());
            }
            let _ = services.settings.set_sources_valid_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            );
            let _ = services.title_cache.invalidate_now();
            load_home(&ui, &services, &pump_dummy(&ui));
            let _ = name;
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_trusted(move |file, trusted| {
            if let Ok(list) = services.extensions.installed() {
                if let Some(ext) = list.iter().find(|e| e.file_name == file.as_str()) {
                    let _ = services.extensions.set_trusted(ext, trusted);
                }
            }
            refresh_plugins(&ui, &services);
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_uninstall_plugin(move |file| {
            if let Ok(list) = services.extensions.installed() {
                if let Some(ext) = list.iter().find(|e| e.file_name == file.as_str()) {
                    let _ = services.extensions.uninstall(ext);
                }
            }
            refresh_plugins(&ui, &services);
        });
    }
    {
        let extras = Rc::clone(&extras);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_clean_storage(move || {
            if let Some(dir) = extras.borrow().data_dir.clone() {
                let _ = std::fs::remove_dir_all(dir.join("sources"));
                let _ = std::fs::remove_dir_all(dir.join("subtitles"));
                let _ = tx_notice(&pump, "Cleaned regenerable caches");
            }
            let _ = ui.upgrade_in_event_loop(|_| {});
        });
    }
    {
        let extras = Rc::clone(&extras);
        some_ui.on_wipe_data(move || {
            if let Some(dir) = extras.borrow().data_dir.clone() {
                let _ = std::fs::write(dir.join("delete_all_data.pending"), b"1");
                let _ = tx_notice_from_extras(&extras, "Data wipe scheduled — restart the app");
            }
        });
    }
    {
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        let extras = Rc::clone(&extras);
        some_ui.on_check_updates(move || {
            let tx = pump.sender();
            let ui = ui.clone();
            let current = env!("CARGO_PKG_VERSION");
            spawn_work("updates", move || {
                match update::check_for_updates(current) {
                    Ok(info) => {
                        let _ = tx.send(UiEvent::UpdateLabel(info.label.clone()));
                        let _ = tx.send(UiEvent::Notice(info.label));
                    }
                    Err(error) => {
                        let _ = tx.send(UiEvent::Notice(format!("Update check failed: {error}")));
                    }
                }
                let _ = ui.upgrade_in_event_loop(|_| {});
            });
            let _ = extras;
        });
    }
    {
        let session = Rc::clone(&session);
        let extras = Rc::clone(&extras);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_apply_update(move || {
            let url = session.borrow().update.as_ref().and_then(|u| u.download_url.clone());
            let Some(url) = url else {
                let _ = tx_notice(&pump, "Check for updates first");
                return;
            };
            let dir = extras
                .borrow()
                .data_dir
                .clone()
                .unwrap_or_else(|| std::env::temp_dir());
            let tx = pump.sender();
            let ui = ui.clone();
            spawn_work("apply-update", move || {
                match update::download_update(&url, &dir.join("updates")) {
                    Ok(path) => match update::apply_downloaded(&path) {
                        Ok(()) => {
                            let _ = tx.send(UiEvent::Notice(
                                "Update staged — the app will restart after exit".into(),
                            ));
                        }
                        Err(error) => {
                            let _ = tx.send(UiEvent::Notice(format!("Apply failed: {error}")));
                        }
                    },
                    Err(error) => {
                        let _ = tx.send(UiEvent::Notice(format!("Download failed: {error}")));
                    }
                }
                let _ = ui.upgrade_in_event_loop(|_| {});
            });
        });
    }
    {
        let extras = Rc::clone(&extras);
        some_ui.on_open_logs(move || {
            if let Some(dir) = extras.borrow().data_dir.as_ref() {
                let _ = open::that(dir.join("logs"));
            }
        });
    }
    {
        let ui = ui.clone();
        some_ui.on_dismiss_notice(move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_notice("".into());
            }
        });
    }
    {
        let ui = ui.clone();
        some_ui.on_resume_details(move || {
            // Resume uses the same play path; episode highlight is already in resume label.
            if let Some(ui) = ui.upgrade() {
                ui.invoke_play_details();
            }
        });
    }
    wire_cloudflare(some_ui, ui, Rc::clone(&session), Rc::clone(&extras));
    let _ = session;
}


