//! Home and details loading.
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

use super::session::Session;
use super::open::begin_open;


pub(crate) fn load_home(ui: &slint::Weak<CloudStream>, services: &UiServices, pump: &EventPump) {
    let services = services.clone();
    let tx = pump.sender();
    let ui = ui.clone();
    let provider = ui
        .upgrade()
        .map(|u| u.get_selected_provider().to_string())
        .unwrap_or_default();
    spawn_work("home", move || {
        let providers = services.extensions.providers();
        let chosen = providers
            .iter()
            .find(|p| p.name == provider)
            .or_else(|| providers.first());
        let mut items = Vec::new();
        if let Some(info) = chosen {
            if let Some(entry) = info.main_page.first() {
                if let Ok(Some(page)) = services.catalog.home(&info.name, entry) {
                    for section in page.items {
                        items.extend(section.list);
                    }
                }
            }
        }
        let _ = tx.send(UiEvent::HomeLoaded(items));
        let _ = ui.upgrade_in_event_loop(|_| {});
    });
}

pub(crate) fn open_home_index(
    ui: &slint::Weak<CloudStream>,
    services: &Rc<UiServices>,
    session: &Rc<RefCell<Session>>,
    pump: &Rc<EventPump>,
    index: i32,
) {
    if let Some(ui_h) = ui.upgrade() {
        if let Some(item) = ui_h.get_home_items().row_data(index as usize) {
            load_details(ui, services, session, pump, &item);
        }
    }
}

pub(crate) fn load_details(
    ui: &slint::Weak<CloudStream>,
    services: &Rc<UiServices>,
    session: &Rc<RefCell<Session>>,
    pump: &Rc<EventPump>,
    item: &PosterItem,
) {
    let provider = item.provider.to_string();
    let url = item.url.to_string();
    let media_key = item.media_key.to_string();
    let title = item.title.to_string();
    {
        let mut s = session.borrow_mut();
        s.details_provider = provider.clone();
        s.details_url = url.clone();
        s.details_media_key = media_key.clone();
        s.details_data = url.clone();
    }
    let services = (**services).clone();
    let tx = pump.sender();
    let ui = ui.clone();
    spawn_work("details", move || {
        match services.catalog.details(&provider, &url) {
            Ok(details) => {
                let resume = services
                    .library
                    .resume_pointer(&provider, &url)
                    .ok()
                    .flatten()
                    .map(|pointer| {
                        match (pointer.season, pointer.episode) {
                            (Some(s), Some(e)) => format!("Resume S{s} E{e}"),
                            _ => "Resume".to_owned(),
                        }
                    })
                    .unwrap_or_default();
                let progress = services
                    .library
                    .show_progress(&provider, &url)
                    .unwrap_or_default();
                let episodes: Vec<(String, String, String, bool)> = details
                    .episodes
                    .iter()
                    .map(|ep| {
                        let code = match (ep.season, ep.episode) {
                            (Some(s), Some(e)) => format!("S{s}E{e}"),
                            _ => String::new(),
                        };
                        let watched = progress.iter().any(|p| p.media_key == ep.data);
                        (
                            ep.name.clone().unwrap_or_else(|| code.clone()),
                            code,
                            ep.data.clone(),
                            watched,
                        )
                    })
                    .collect();
                let data = if details.episodes.is_empty() {
                    details.data_url.clone().unwrap_or(url.clone())
                } else {
                    url.clone()
                };
                let year = details.year.map(|y| y.to_string()).unwrap_or_default();
                let _ = tx.send(UiEvent::DetailsLoaded {
                    title: if details.name.is_empty() {
                        title
                    } else {
                        details.name
                    },
                    plot: details.plot.unwrap_or_default(),
                    meta: year,
                    resume,
                    episodes,
                    provider,
                    url,
                    data,
                    media_key,
                });
            }
            Err(error) => {
                let _ = tx.send(UiEvent::DetailsFailed(error.to_string()));
            }
        }
        let _ = ui.upgrade_in_event_loop(|_| {});
    });
}


