//! Opens a video: search and check its streams, play one, refresh or switch the source.

use std::sync::atomic::{AtomicBool, Ordering};

use app_services::{PlaybackItem, StreamSelection, UiServices};
use playback_session::{
    run_open, CheckedOpen, LiveSources, OpenControl, OpenProgress, OpenRequest, RefreshRequest,
};

use crate::background_work::{spawn_work, UiSender};
use crate::posters::source_rows;
use crate::request_tickets::{RequestKind, Ticket};
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;

/// What a worker needs to open one video.
struct OpenJob {
    ticket: Ticket,
    services: UiServices,
    item: PlaybackItem,
    data: String,
    control: OpenControl,
    live: LiveSources,
}

/// Starts finding streams for `item` (`data` is the provider's episode or movie payload).
pub(crate) fn begin_open(ctx: &BridgeContext, item: PlaybackItem, data: String) {
    if ctx.services.playback.is_none() {
        let problem = ctx.services.player_problem.clone();
        ctx.sender()
            .notice(problem.unwrap_or_else(|| "no player".into()));
        return;
    }
    let (control, live, ticket) = {
        let mut session = ctx.session.borrow_mut();
        // The video opened before stops searching and checking; its answers are dropped.
        session.control.cancel_open();
        let ticket = session.tickets.issue(RequestKind::Open);
        session.control = OpenControl::default();
        session.live = LiveSources::new();
        session.target = Some(item.clone());
        session.episode_data.clone_from(&data);
        session.links.clear();
        session.selected = 0;
        (session.control.clone(), session.live.clone(), ticket)
    };
    if let Some(ui) = ctx.window() {
        ui.set_open_busy(true);
        ui.set_open_status("Searching streams…".into());
        ui.set_player_title(item.title.clone().into());
    }
    let job = OpenJob {
        ticket,
        services: ctx.owned_services(),
        item,
        data,
        control,
        live,
    };
    let sender = ctx.sender();
    spawn_work("open-streams", move || run_open_job(&job, &sender));
}

fn run_open_job(job: &OpenJob, sender: &UiSender) {
    let ticket = job.ticket;
    let ready_sent = AtomicBool::new(false);
    let on_progress = |progress: OpenProgress| sender.send(UiEvent::OpenProgress(ticket, progress));
    let on_update = |open: &CheckedOpen| {
        job.live.publish(open.clone());
        sender.send(UiEvent::OpenUpdate(ticket, open.clone()));
    };
    let on_ready = |index: usize| {
        if !ready_sent.swap(true, Ordering::SeqCst) {
            sender.send(UiEvent::OpenReady(ticket, index));
        }
    };
    let request = OpenRequest {
        streams: &job.services.streams,
        provider: &job.item.provider,
        data: &job.data,
        media_key: &job.item.media_key,
        cache: Some(&job.services.title_cache),
        on_progress: &on_progress,
        on_update: &on_update,
        on_ready: &on_ready,
        control: &job.control,
    };
    match run_open(request) {
        Ok(opened) => {
            sender.send(UiEvent::OpenUpdate(ticket, opened.clone()));
            match opened.play_index {
                Some(index) => on_ready(index),
                None if !ready_sent.load(Ordering::SeqCst) => {
                    sender.send(UiEvent::OpenFailed(ticket, "no working streams".into()));
                }
                None => {}
            }
        }
        Err(error) => sender.send(UiEvent::OpenFailed(ticket, error.to_string())),
    }
}

/// Searches the playing title's streams again while the current one keeps playing.
pub(crate) fn refresh_playing(ctx: &BridgeContext) {
    let session = ctx.session.borrow();
    let job = OpenJob {
        ticket: session.tickets.current(RequestKind::Open),
        services: ctx.owned_services(),
        item: session
            .target
            .clone()
            .unwrap_or_else(|| empty_item(&session.details.provider)),
        data: session.episode_data.clone(),
        control: session.control.clone(),
        live: session.live.clone(),
    };
    let playing_url = session
        .links
        .get(session.selected)
        .map(|link| link.url.clone());
    drop(session);
    let sender = ctx.sender();
    spawn_work("refresh-sources", move || {
        let ticket = job.ticket;
        let on_progress =
            |progress: OpenProgress| sender.send(UiEvent::OpenProgress(ticket, progress));
        let request = RefreshRequest {
            streams: &job.services.streams,
            provider: &job.item.provider,
            data: &job.data,
            media_key: &job.item.media_key,
            playing_url: playing_url.as_deref(),
            cache: Some(&job.services.title_cache),
            live: &job.live,
            control: &job.control,
            on_progress: &on_progress,
        };
        match playback_session::refresh_sources(request) {
            Ok(opened) => sender.send(UiEvent::OpenUpdate(ticket, opened)),
            Err(error) => sender.notice(format!("Refresh failed: {error}")),
        }
    });
}

fn empty_item(provider: &str) -> PlaybackItem {
    PlaybackItem {
        provider: provider.to_owned(),
        title: String::new(),
        media_key: String::new(),
        detail_url: String::new(),
        poster_url: None,
        season: None,
        episode: None,
        next_episode: None,
    }
}

/// Plays stream `index` of the open list from `start_position_ms`.
fn play_stream(ctx: &BridgeContext, index: usize, start_position_ms: i64) -> Result<(), String> {
    let Some(playback) = &ctx.services.playback else {
        return Err("no player".into());
    };
    let (item, link, subtitles) = {
        let session = ctx.session.borrow();
        let item = session.target.clone().ok_or("nothing is open")?;
        let link = session
            .links
            .get(index)
            .cloned()
            .ok_or("that source is gone")?;
        (item, link, session.subtitles.clone())
    };
    let stream = StreamSelection {
        link: &link,
        subtitles: &subtitles,
        start_position_ms,
    };
    playback
        .start_at(item, &stream)
        .map_err(|error| error.to_string())
}

/// Starts the stream the opening picked.
pub(crate) fn start_selected(ctx: &BridgeContext, ui: &CloudStream, index: usize) {
    if let Err(error) = play_stream(ctx, index, 0) {
        tracing::warn!(%error, index, "the picked stream did not start");
        ui.set_notice(format!("Cannot start playback: {error}").into());
    }
}

/// Plays another source of the list from the current position.
pub(crate) fn switch_source(ctx: &BridgeContext, index: usize) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let position = ui.get_position_ms() as i64;
    match play_stream(ctx, index, position) {
        Ok(()) => {
            ctx.session.borrow_mut().selected = index;
            let snapshot = ctx.session.borrow().live.snapshot();
            ui.set_sources(source_rows(&snapshot.links, Some(index)));
        }
        Err(error) => {
            tracing::warn!(%error, index, "switching the source failed");
            ui.set_notice(format!("Cannot switch source: {error}").into());
        }
    }
}
