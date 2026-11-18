//! Drain background events and poll the player.
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
use super::open::start_selected;
use super::settings_extra::refresh_plugins;


pub(crate) fn drain_events(
    ui: &CloudStream,
    services: &UiServices,
    session: &RefCell<Session>,
    pump: &EventPump,
) {
    for event in pump.drain() {
        match event {
            UiEvent::StartupProgress {
                finished,
                total,
                current,
            } => {
                let progress = if total == 0 {
                    1.0
                } else {
                    finished as f32 / total as f32
                };
                ui.set_startup_progress(progress);
                ui.set_startup_status(format!("Loading {current} ({finished}/{total})").into());
            }
            UiEvent::StartupDone { providers, error } => {
                ui.set_starting(false);
                ui.set_providers(ModelRc::new(VecModel::from(
                    providers
                        .iter()
                        .map(|p| slint::SharedString::from(p.as_str()))
                        .collect::<Vec<_>>(),
                )));
                if let Some(first) = providers.first() {
                    ui.set_selected_provider(first.into());
                }
                if let Some(error) = error {
                    ui.set_notice(error.into());
                }
                refresh_plugins(&ui.as_weak(), services);
                if let Ok(bookmarks) = services.library.bookmarks() {
                    ui.set_library_items(posters_from_bookmarks(&bookmarks));
                }
                if let Ok(cont) = services.library.continue_watching() {
                    ui.set_continue_items(posters_from_continue(&cont));
                }
                load_home(&ui.as_weak(), services, pump);
            }
            UiEvent::HomeLoaded(items) => ui.set_home_items(posters_from_search(&items)),
            UiEvent::SearchLoaded(items) => ui.set_search_items(posters_from_search(&items)),
            UiEvent::BrowseLoaded(rows) => {
                let posters: Vec<PosterItem> = rows
                    .into_iter()
                    .map(|row| PosterItem {
                        title: row.title.into(),
                        subtitle: row.subtitle.into(),
                        provider: ui.get_selected_provider(),
                        url: String::new().into(),
                        poster: String::new().into(),
                        media_key: String::new().into(),
                        data: String::new().into(),
                    })
                    .collect();
                ui.set_browse_items(ModelRc::new(VecModel::from(posters)));
            }
            UiEvent::DetailsLoaded {
                title,
                plot,
                meta,
                resume,
                episodes,
                provider,
                url,
                data,
                media_key,
            } => {
                {
                    let mut s = session.borrow_mut();
                    s.details_provider = provider;
                    s.details_url = url;
                    s.details_data = data;
                    s.details_media_key = media_key;
                }
                ui.set_details_title(title.into());
                ui.set_details_plot(plot.into());
                ui.set_details_meta(meta.into());
                ui.set_resume_label(resume.into());
                let rows: Vec<EpisodeItem> = episodes
                    .into_iter()
                    .map(|(name, code, data, watched)| EpisodeItem {
                        name: name.into(),
                        code: code.into(),
                        data: data.into(),
                        watched,
                    })
                    .collect();
                ui.set_episodes(ModelRc::new(VecModel::from(rows)));
            }
            UiEvent::DetailsFailed(error) => ui.set_notice(error.into()),
            UiEvent::OpenProgress {
                found,
                checked,
                working,
                search_done,
            } => {
                let done = if search_done { " · search done" } else { "" };
                ui.set_open_status(
                    format!("Streams {found} · checked {checked} · working {working}{done}")
                        .into(),
                );
                ui.set_status_line(ui.get_open_status());
            }
            UiEvent::OpenReady { index } => {
                session.borrow_mut().selected = index;
                ui.set_player_open(true);
                ui.set_open_busy(false);
                start_selected(ui, services, session, index);
            }
            UiEvent::OpenUpdate {
                links,
                play_index,
                subtitles,
            } => {
                {
                    let mut s = session.borrow_mut();
                    s.links = links.iter().map(|(l, _)| l.clone()).collect();
                    s.subtitles = subtitles;
                    if let Some(index) = play_index {
                        s.selected = index;
                    }
                    s.live.publish(playback_session::CheckedOpen {
                        links: links.clone(),
                        play_index,
                        subtitles: s.subtitles.clone(),
                    });
                }
                ui.set_sources(source_rows(&links, play_index));
            }
            UiEvent::OpenFailed(error) => {
                ui.set_open_busy(false);
                ui.set_notice(error.into());
            }
            UiEvent::Notice(text) => ui.set_notice(text.into()),
            UiEvent::PlaybackTick {
                position_ms,
                duration_ms,
                paused,
            } => {
                ui.set_position_ms(position_ms);
                ui.set_duration_ms(duration_ms.max(1.0));
                ui.set_paused(paused);
            }
            UiEvent::CloudflareNeeded { host, url, id } => {
                session.borrow_mut().cf_host = Some(host.clone());
                session.borrow_mut().cf_id = Some(id);
                ui.set_cf_host(host.into());
                ui.set_cf_url(url.into());
                ui.set_show_cf(true);
            }
            UiEvent::UpdateLabel(label) => ui.set_update_label(label.into()),
            UiEvent::StorageLabel(label) => ui.set_storage_label(label.into()),
        }
    }
}

pub(crate) fn poll_player(ui: &CloudStream, services: &UiServices, session: &RefCell<Session>) {
    let Some(playback) = &services.playback else {
        return;
    };
    if !ui.get_player_open() {
        return;
    }
    let status = playback.tick();
    ui.set_position_ms(status.position_ms as f32);
    ui.set_duration_ms(status.duration_ms.max(1) as f32);
    ui.set_paused(status.state == player_api::PlaybackState::Paused);
    let mut s = session.borrow_mut();
    if let Some(frame) = playback.latest_frame(s.last_frame) {
        s.last_frame = frame.serial;
        let buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            &frame.rgba,
            frame.width,
            frame.height,
        );
        ui.set_video_frame(Image::from_rgba8(buffer));
    }
    // Auto-advance
    if status.state == player_api::PlaybackState::Ended
        && services.settings.autoplay_next().unwrap_or(true)
    {
        if let Some(target) = s.target.as_ref().and_then(|t| t.next_episode.clone()) {
            let _ = target;
        }
    }
}

pub(crate) fn drain_cloudflare(
    ui: &CloudStream,
    extras: &RefCell<AppExtras>,
    session: &RefCell<Session>,
    pump: &EventPump,
) {
    let mut extras = extras.borrow_mut();
    let Some(watch) = extras.cloudflare.as_ref() else {
        return;
    };
    let mut pending = Vec::new();
    while let Ok(event) = watch.events.try_recv() {
        pending.push(event);
    }
    let data_dir = extras.data_dir.clone();
    drop(extras);
    for event in pending {
        match event {
            cloudflare_bridge::CloudflareEvent::Needed(request) => {
                let allowed = session.borrow().allowed_cf.contains(&request.host);
                if allowed {
                    let _ = open::that(&request.url);
                    if let Some(dir) = data_dir.as_ref() {
                        let dir = dir.join("sidecar/cloudflare");
                        let _ = cloudflare_bridge::write_result(
                            &dir,
                            &request.id,
                            &cloudflare_bridge::Outcome::Waiting(
                                "complete verification in browser".into(),
                            ),
                        );
                    }
                    ui.set_notice(
                        format!("Cloudflare: complete check for {}", request.host).into(),
                    );
                } else {
                    let _ = pump.sender().send(UiEvent::CloudflareNeeded {
                        host: request.host,
                        url: request.url,
                        id: request.id,
                    });
                }
            }
        }
    }
}

pub(crate) fn browse_tmdb(kind: &str) -> Result<Vec<BrowseRow>, String> {
    use app_services::{BrowseFilterRequest, BrowseService};
    let client = BrowseService::default();
    let request = BrowseFilterRequest {
        media_type: kind.to_owned(),
        year: None,
        sort_by: "popular".into(),
        rating: None,
        genres: Vec::new(),
        genre: None,
        page: 1,
    };
    let page = client.fetch_browse_page(&request)?;
    Ok(page
        .items
        .into_iter()
        .map(|item| BrowseRow {
            title: item.name,
            subtitle: item.media_type,
            year: item.year,
        })
        .collect())
}

