//! Callback wiring.
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
use super::settings::wire_settings;
use super::catalog::{load_details, open_home_index};
use super::events::browse_tmdb;
use super::open::{begin_open, refresh_playing, switch_source};

pub(crate) fn wire_callbacks(
    ui: slint::Weak<CloudStream>,
    services: Rc<UiServices>,
    session: Rc<RefCell<Session>>,
    pump: Rc<EventPump>,
    extras: Rc<RefCell<AppExtras>>,
) {
    let Some(some_ui) = ui.upgrade() else {
        return;
    };

    {
        let ui = ui.clone();
        some_ui.on_select_tab(move |tab| {
            if let Some(ui) = ui.upgrade() {
                ui.set_tab(tab);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_search(move |query| {
            let q = query.to_string();
            let services = Rc::clone(&services);
            let tx = pump.sender();
            let ui = ui.clone();
            let services = (*services).clone();
            spawn_work("search", move || {
                let providers = services.extensions.providers();
                let mut items = Vec::new();
                for outcome in services.catalog.search(&providers, &q) {
                    if let Ok(page) = outcome.result {
                        items.extend(page.items);
                    }
                }
                let _ = tx.send(UiEvent::SearchLoaded(items));
                let _ = ui.upgrade_in_event_loop(|_| {});
            });
        });
    }
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_open_poster(move |index| {
            open_home_index(&ui, &services, &session, &pump, index);
        });
    }
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_open_search_hit(move |index| {
            if let Some(ui_h) = ui.upgrade() {
                if let Some(item) = ui_h.get_search_items().row_data(index as usize) {
                    load_details(&ui, &services, &session, &pump, &item);
                }
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_open_browse_hit(move |index| {
            if let Some(ui_h) = ui.upgrade() {
                if let Some(item) = ui_h.get_browse_items().row_data(index as usize) {
                    load_details(&ui, &services, &session, &pump, &item);
                }
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_open_library_hit(move |index| {
            if let Some(ui_h) = ui.upgrade() {
                let model = if ui_h.get_tab() == 0 {
                    ui_h.get_continue_items()
                } else {
                    ui_h.get_library_items()
                };
                if let Some(item) = model.row_data(index as usize) {
                    load_details(&ui, &services, &session, &pump, &item);
                }
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_play_details(move || {
            let (provider, data, media_key, item) = {
                let s = session.borrow();
                let item = PlaybackItem {
                    provider: s.details_provider.clone(),
                    media_key: s.details_media_key.clone(),
                    title: ui.upgrade().map(|u| u.get_details_title().to_string()).unwrap_or_default(),
                    detail_url: s.details_url.clone(),
                    poster_url: None,
                    season: None,
                    episode: None,
                    next_episode: None,
                };
                (s.details_provider.clone(), s.details_data.clone(), s.details_media_key.clone(), item)
            };
            begin_open(&ui, &services, &session, &pump, item, data);
            let _ = (provider, media_key);
        });
    }
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_play_episode(move |index| {
            if let Some(ui_h) = ui.upgrade() {
                if let Some(ep) = ui_h.get_episodes().row_data(index as usize) {
                    let item = {
                        let s = session.borrow();
                        PlaybackItem {
                            provider: s.details_provider.clone(),
                            media_key: ep.data.to_string(),
                            title: format!("{} — {}", ui_h.get_details_title(), ep.name),
                            detail_url: s.details_url.clone(),
                            poster_url: None,
                            season: None,
                            episode: None,
                            next_episode: None,
                        }
                    };
                    begin_open(&ui, &services, &session, &pump, item, ep.data.to_string());
                }
            }
        });
    }
    {
        let ui = ui.clone();
        some_ui.on_close_details(move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_details_title("".into());
                ui.set_episodes(empty_episodes());
            }
        });
    }
    {
        let session = Rc::clone(&session);
        some_ui.on_skip_checks(move || {
            session.borrow().control.skip_checks();
        });
    }
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_refresh_sources(move || {
            refresh_playing(&ui, &services, &session, &pump);
        });
    }
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let ui = ui.clone();
        some_ui.on_select_source(move |index| {
            switch_source(&ui, &services, &session, index as usize);
        });
    }
    {
        let services = Rc::clone(&services);
        some_ui.on_toggle_pause(move || {
            if let Some(playback) = &services.playback {
                let status = playback.tick();
                let paused = status.state == player_api::PlaybackState::Paused;
                let _ = playback.set_paused(!paused);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        some_ui.on_seek_to(move |ms| {
            if let Some(playback) = &services.playback {
                let _ = playback.seek_to(ms as i64);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        some_ui.on_set_volume(move |vol| {
            if let Some(playback) = &services.playback {
                let _ = playback.set_volume(vol.clamp(0.0, 130.0) as u8);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_close_player(move || {
            if let Some(playback) = &services.playback {
                let _ = playback.stop();
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_player_open(false);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_leave_player(move || {
            if let Some(playback) = &services.playback {
                if services.settings.keep_video_ready().unwrap_or(true) {
                    if let Ok(cache) = services.settings.video_cache() {
                        let _ = playback.leave(cache.while_away());
                    }
                } else {
                    let _ = playback.stop();
                }
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_player_open(false);
            }
        });
    }
    {
        let ui = ui.clone();
        some_ui.on_toggle_sources(move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_show_sources(!ui.get_show_sources());
            }
        });
    }
    {
        let services = Rc::clone(&services);
        some_ui.on_skip_segment(move || {
            if let Some(playback) = &services.playback {
                let status = playback.tick();
                let _ = playback.seek_to(status.position_ms + 85_000);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_load_browse(move |kind| {
            let kind = kind.to_string();
            let services = Rc::clone(&services);
            let tx = pump.sender();
            let ui = ui.clone();
            spawn_work("browse", move || {
                let rows = browse_tmdb(&kind).unwrap_or_default();
                let _ = tx.send(UiEvent::BrowseLoaded(rows));
                let _ = ui.upgrade_in_event_loop(|_| {});
            });
            let _ = services;
        });
    }
    wire_settings(&some_ui, ui.clone(), Rc::clone(&services), Rc::clone(&session), Rc::clone(&pump), Rc::clone(&extras));
}



