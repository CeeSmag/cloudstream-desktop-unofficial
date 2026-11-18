//! Extra Slint callbacks for shelves, hero, seasons, and player tracks.

use std::cell::RefCell;
use std::rc::Rc;

use app_services::UiServices;
use slint::{ComponentHandle, Model};

use crate::runner::EventPump;
use crate::CloudStream;

use super::catalog::{open_shelf_item, parse_season_label};
use super::session::Session;

pub(crate) fn wire_extra(
    some_ui: &CloudStream,
    ui: slint::Weak<CloudStream>,
    services: Rc<UiServices>,
    session: Rc<RefCell<Session>>,
    pump: Rc<EventPump>,
) {
    {
        let services = Rc::clone(&services);
        let session = Rc::clone(&session);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_open_shelf_item(move |shelf, index| {
            open_shelf_item(&ui, &services, &session, &pump, shelf, index);
        });
    }
    {
        let ui = ui.clone();
        some_ui.on_resume_hero(move || {
            if let Some(ui) = ui.upgrade() {
                if ui.get_continue_items().row_count() > 0 {
                    ui.invoke_open_library_hit(0);
                }
            }
        });
    }
    {
        let ui = ui.clone();
        some_ui.on_play_continue(move |index| {
            if let Some(ui) = ui.upgrade() {
                ui.invoke_open_library_hit(index);
            }
        });
    }
    {
        some_ui.on_bookmark_continue(move |_index| {});
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_remove_continue(move |index| {
            if let Some(ui_h) = ui.upgrade() {
                if let Some(item) = ui_h.get_continue_items().row_data(index as usize) {
                    let _ = services
                        .library
                        .remove_continue_watching(&item.provider, &item.url.to_string());
                    if let Ok(cont) = services.library.continue_watching() {
                        ui_h.set_continue_items(crate::posters::posters_from_continue(&cont));
                    }
                }
            }
        });
    }
    {
        let ui = ui.clone();
        some_ui.on_season_chosen(move |label| {
            if let Some(ui) = ui.upgrade() {
                ui.set_season_filter(parse_season_label(&label));
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_select_audio_track(move |label| {
            if let Some(playback) = &services.playback {
                if let Some(track) = playback.tracks().into_iter().find(|t| {
                    t.kind == player_api::TrackKind::Audio && t.label() == label.as_str()
                }) {
                    let _ = playback.select_track(player_api::TrackKind::Audio, Some(track.id));
                }
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_audio_track_value(label);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_select_subtitle_track(move |label| {
            if let Some(playback) = &services.playback {
                if label == "Off" {
                    let _ = playback.select_track(player_api::TrackKind::Subtitle, None);
                } else if let Some(track) = playback.tracks().into_iter().find(|t| {
                    t.kind == player_api::TrackKind::Subtitle && t.label() == label.as_str()
                }) {
                    let _ = playback.select_track(player_api::TrackKind::Subtitle, Some(track.id));
                }
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_subtitle_track_value(label);
            }
        });
    }
    {
        let ui = ui.clone();
        some_ui.on_reveal_player_chrome(move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_player_chrome_visible(true);
            }
        });
    }
    {
        some_ui.on_bookmark_item(move |_provider, _url, _title| {});
    }
}
