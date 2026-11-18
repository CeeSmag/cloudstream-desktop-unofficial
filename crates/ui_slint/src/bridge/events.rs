//! Drain background events and poll the player.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use app_services::{PlaybackItem, StreamSelection, UiServices};
use playback_session::{run_open, LiveSources, OpenControl, OpenRequest, RefreshRequest};
use slint::{ComponentHandle, Image, Model, ModelRc, SharedPixelBuffer, VecModel};

use crate::poster_images::global_cache;
use crate::posters::{
    posters_from_bookmarks, posters_from_continue, posters_from_search, shelves_from_sections,
    source_rows, string_model,
};
use crate::runner::{BrowseRow, EventPump, PosterKind, UiEvent};
use crate::{CloudStream, EpisodeItem, PosterItem};

use super::catalog::{load_home, season_labels_from_episodes};
use super::open::start_selected;
use super::poster_hydrate::hydrate_poster_images;
use super::session::{AppExtras, Session};
use super::settings_extra::refresh_plugins;

pub(crate) use super::events_extra::{browse_tmdb, drain_cloudflare};


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
                    hydrate_poster_images(&ui.as_weak(), &global_cache(), PosterKind::Library);
                }
                if let Ok(cont) = services.library.continue_watching() {
                    ui.set_continue_items(posters_from_continue(&cont));
                    if let Some(first) = cont.first() {
                        ui.set_hero_title(first.title.clone().into());
                        let subtitle = match (first.season, first.episode) {
                            (Some(s), Some(e)) => format!("S{s} E{e}"),
                            _ => first.provider.clone(),
                        };
                        ui.set_hero_subtitle(subtitle.into());
                    }
                    hydrate_poster_images(&ui.as_weak(), &global_cache(), PosterKind::Continue);
                }
                load_home(&ui.as_weak(), services, pump);
            }
            UiEvent::HomeLoaded(sections) => {
                let flat: Vec<core_model::SearchResult> = sections
                    .iter()
                    .flat_map(|(_, items)| items.iter().cloned())
                    .collect();
                ui.set_home_items(posters_from_search(&flat));
                ui.set_home_shelves(shelves_from_sections(&sections));
                hydrate_poster_images(&ui.as_weak(), &global_cache(), PosterKind::Shelf);
            }
            UiEvent::SearchLoaded(items) => {
                ui.set_search_items(posters_from_search(&items));
                hydrate_poster_images(&ui.as_weak(), &global_cache(), PosterKind::Search);
            }
            UiEvent::BrowseLoaded(rows) => {
                let posters: Vec<PosterItem> = rows
                    .into_iter()
                    .map(|row| PosterItem {
                        title: row.title.clone().into(),
                        subtitle: row.subtitle.clone().into(),
                        provider: ui.get_selected_provider(),
                        url: row.url.clone().into(),
                        poster: row.poster.clone().into(),
                        media_key: row.url.clone().into(),
                        data: row.url.into(),
                        image: slint::Image::default(),
                        progress: 0.0,
                    })
                    .collect();
                ui.set_browse_items(ModelRc::new(VecModel::from(posters)));
                hydrate_poster_images(&ui.as_weak(), &global_cache(), PosterKind::Browse);
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
                ui.set_season_filter(-1);
                ui.set_season_labels(string_model(season_labels_from_episodes(&episodes)));
                let rows: Vec<EpisodeItem> = episodes
                    .into_iter()
                    .map(|(name, code, data, watched, season)| EpisodeItem {
                        name: name.into(),
                        code: code.into(),
                        data: data.into(),
                        watched,
                        season,
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
    let tracks = playback.tracks();
    let audio_labels: Vec<String> = tracks
        .iter()
        .filter(|t| t.kind == player_api::TrackKind::Audio)
        .map(player_api::MediaTrack::label)
        .collect();
    let mut sub_labels: Vec<String> = vec!["Off".into()];
    sub_labels.extend(
        tracks
            .iter()
            .filter(|t| t.kind == player_api::TrackKind::Subtitle)
            .map(player_api::MediaTrack::label),
    );
    ui.set_audio_track_labels(string_model(audio_labels));
    ui.set_subtitle_track_labels(string_model(sub_labels));
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

