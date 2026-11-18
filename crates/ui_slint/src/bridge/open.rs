//! Stream open / refresh / switch.
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

pub(crate) fn begin_open(
    ui: &slint::Weak<CloudStream>,
    services: &Rc<UiServices>,
    session: &Rc<RefCell<Session>>,
    pump: &Rc<EventPump>,
    item: PlaybackItem,
    data: String,
) {
    if services.playback.is_none() {
        let _ = pump.sender().send(UiEvent::Notice(
            services
                .player_problem
                .clone()
                .unwrap_or_else(|| "no player".into()),
        ));
        return;
    }
    {
        let mut s = session.borrow_mut();
        s.control = OpenControl::default();
        s.live = LiveSources::new();
        s.target = Some(item.clone());
        s.episode_data = data.clone();
        s.links.clear();
        s.selected = 0;
    }
    if let Some(ui) = ui.upgrade() {
        ui.set_open_busy(true);
        ui.set_open_status("Searching streams…".into());
        ui.set_player_title(item.title.clone().into());
    }
    let streams = Arc::clone(&services.streams);
    let title_cache = Arc::clone(&services.title_cache);
    let control = session.borrow().control.clone();
    let live = session.borrow().live.clone();
    let tx = pump.sender();
    let ui = ui.clone();
    let provider = item.provider.clone();
    let media_key = item.media_key.clone();
    spawn_work("open-streams", move || {
        let on_progress = |progress: playback_session::OpenProgress| {
            let _ = tx.send(UiEvent::OpenProgress {
                found: progress.found,
                checked: progress.checked,
                working: progress.working,
                search_done: progress.search_done,
            });
            let _ = ui.upgrade_in_event_loop(|_| {});
        };
        let on_update = |open: &playback_session::CheckedOpen| {
            live.publish(open.clone());
            let _ = tx.send(UiEvent::OpenUpdate {
                links: open.links.clone(),
                play_index: open.play_index,
                subtitles: open.subtitles.clone(),
            });
            let _ = ui.upgrade_in_event_loop(|_| {});
        };
        let ready_sent = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ready_flag = Arc::clone(&ready_sent);
        let tx_ready = tx.clone();
        let ui_ready = ui.clone();
        let on_ready = move |index: usize| {
            if ready_flag.swap(true, Ordering::SeqCst) {
                return;
            }
            let _ = tx_ready.send(UiEvent::OpenReady { index });
            let _ = ui_ready.upgrade_in_event_loop(|_| {});
        };
        let request = OpenRequest {
            streams: &streams,
            provider: &provider,
            data: &data,
            media_key: &media_key,
            cache: Some(&title_cache),
            on_progress: &on_progress,
            on_update: &on_update,
            on_ready: &on_ready,
            control: &control,
        };
        match run_open(request) {
            Ok(opened) => {
                let _ = tx.send(UiEvent::OpenUpdate {
                    links: opened.links.clone(),
                    play_index: opened.play_index,
                    subtitles: opened.subtitles.clone(),
                });
                if !ready_sent.load(Ordering::SeqCst) {
                    if let Some(index) = opened.play_index {
                        on_ready(index);
                    } else {
                        let _ = tx.send(UiEvent::OpenFailed("no working streams".into()));
                    }
                }
            }
            Err(error) => {
                let _ = tx.send(UiEvent::OpenFailed(error.to_string()));
            }
        }
        let _ = ui.upgrade_in_event_loop(|_| {});
    });
}

pub(crate) fn refresh_playing(
    ui: &slint::Weak<CloudStream>,
    services: &Rc<UiServices>,
    session: &Rc<RefCell<Session>>,
    pump: &Rc<EventPump>,
) {
    let (provider, data, media_key, playing_url, control, live) = {
        let s = session.borrow();
        let playing_url = s.links.get(s.selected).map(|l| l.url.clone());
        (
            s.details_provider.clone(),
            s.episode_data.clone(),
            s.target
                .as_ref()
                .map(|t| t.media_key.clone())
                .unwrap_or_default(),
            playing_url,
            s.control.clone(),
            s.live.clone(),
        )
    };
    let streams = Arc::clone(&services.streams);
    let title_cache = Arc::clone(&services.title_cache);
    let tx = pump.sender();
    let ui = ui.clone();
    spawn_work("refresh-sources", move || {
        let on_progress = |progress: playback_session::OpenProgress| {
            let _ = tx.send(UiEvent::OpenProgress {
                found: progress.found,
                checked: progress.checked,
                working: progress.working,
                search_done: progress.search_done,
            });
            let _ = ui.upgrade_in_event_loop(|_| {});
        };
        let request = RefreshRequest {
            streams: &streams,
            provider: &provider,
            data: &data,
            media_key: &media_key,
            playing_url: playing_url.as_deref(),
            cache: Some(&title_cache),
            live: &live,
            control: &control,
            on_progress: &on_progress,
        };
        match playback_session::refresh_sources(request) {
            Ok(opened) => {
                let _ = tx.send(UiEvent::OpenUpdate {
                    links: opened.links,
                    play_index: opened.play_index,
                    subtitles: opened.subtitles,
                });
            }
            Err(error) => {
                let _ = tx.send(UiEvent::Notice(format!("Refresh failed: {error}")));
            }
        }
        let _ = ui.upgrade_in_event_loop(|_| {});
    });
}

pub(crate) fn start_selected(
    ui: &CloudStream,
    services: &UiServices,
    session: &RefCell<Session>,
    index: usize,
) {
    let Some(playback) = &services.playback else {
        return;
    };
    let (item, link, subs) = {
        let s = session.borrow();
        let Some(item) = s.target.clone() else {
            return;
        };
        let Some(link) = s.links.get(index).cloned() else {
            return;
        };
        (item, link, s.subtitles.clone())
    };
    let stream = StreamSelection {
        link: &link,
        subtitles: &subs,
        start_position_ms: 0,
    };
    if let Err(error) = playback.start_at(item, &stream) {
        ui.set_notice(format!("Cannot start playback: {error}").into());
    }
}

pub(crate) fn switch_source(
    ui: &slint::Weak<CloudStream>,
    services: &Rc<UiServices>,
    session: &Rc<RefCell<Session>>,
    index: usize,
) {
    let Some(playback) = &services.playback else {
        return;
    };
    let (item, link, subs, position) = {
        let s = session.borrow();
        let Some(item) = s.target.clone() else {
            return;
        };
        let Some(link) = s.links.get(index).cloned() else {
            return;
        };
        let position = ui
            .upgrade()
            .map(|u| u.get_position_ms() as i64)
            .unwrap_or(0);
        (item, link, s.subtitles.clone(), position)
    };
    let stream = StreamSelection {
        link: &link,
        subtitles: &subs,
        start_position_ms: position,
    };
    if playback.start_at(item, &stream).is_ok() {
        session.borrow_mut().selected = index;
        if let Some(ui) = ui.upgrade() {
            let snap = session.borrow().live.snapshot();
            ui.set_sources(source_rows(&snap.links, Some(index)));
        }
    }
}


