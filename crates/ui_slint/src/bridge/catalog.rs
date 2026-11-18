//! Home and details loading.
use std::cell::RefCell;
use std::rc::Rc;

use app_services::UiServices;
use slint::{ComponentHandle, Model};

use crate::posters::poster_from_search;
use crate::runner::{spawn_work, EventPump, UiEvent};
use crate::{CloudStream, PosterItem};

use super::session::Session;

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
        let mut sections = Vec::new();
        if let Some(info) = chosen {
            for entry in &info.main_page {
                if let Ok(Some(page)) = services.catalog.home(&info.name, entry) {
                    for section in page.items {
                        let title = if section.name.is_empty() {
                            entry.name.clone()
                        } else {
                            section.name
                        };
                        sections.push((title, section.list));
                    }
                }
            }
        }
        let _ = tx.send(UiEvent::HomeLoaded(sections));
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

pub(crate) fn open_shelf_item(
    ui: &slint::Weak<CloudStream>,
    services: &Rc<UiServices>,
    session: &Rc<RefCell<Session>>,
    pump: &Rc<EventPump>,
    shelf: i32,
    index: i32,
) {
    let Some(ui_h) = ui.upgrade() else {
        return;
    };
    let Some(section) = ui_h.get_home_shelves().row_data(shelf as usize) else {
        return;
    };
    let Some(item) = section.items.row_data(index as usize) else {
        return;
    };
    load_details(ui, services, session, pump, &item);
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
                    .map(|pointer| match (pointer.season, pointer.episode) {
                        (Some(s), Some(e)) => format!("Resume S{s} E{e}"),
                        _ => "Resume".to_owned(),
                    })
                    .unwrap_or_default();
                let progress = services
                    .library
                    .show_progress(&provider, &url)
                    .unwrap_or_default();
                let episodes: Vec<(String, String, String, bool, i32)> = details
                    .episodes
                    .iter()
                    .map(|ep| {
                        let season = ep.season.unwrap_or(-1);
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
                            season,
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

pub(crate) fn season_labels_from_episodes(
    episodes: &[(String, String, String, bool, i32)],
) -> Vec<String> {
    let mut labels = vec!["All".to_owned()];
    let mut seen = std::collections::BTreeSet::new();
    for (_, _, _, _, season) in episodes {
        if *season > 0 {
            seen.insert(*season);
        }
    }
    for season in seen {
        labels.push(format!("Season {season}"));
    }
    labels
}

pub(crate) fn parse_season_label(label: &str) -> i32 {
    if label == "All" {
        return -1;
    }
    label
        .strip_prefix("Season ")
        .and_then(|n| n.parse().ok())
        .unwrap_or(-1)
}

#[allow(dead_code)]
pub(crate) fn flatten_home_items(
    sections: &[(String, Vec<core_model::SearchResult>)],
) -> Vec<PosterItem> {
    sections
        .iter()
        .flat_map(|(_, items)| items.iter().map(poster_from_search))
        .collect()
}
