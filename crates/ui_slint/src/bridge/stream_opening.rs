//! Opens a video: search and check its streams, play one, refresh or switch the source.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_services::{full_label, PlaybackItem, StreamSelection, UiServices};
use playback_session::{
    run_open, title_key, CheckedOpen, LiveSources, OpenControl, OpenProgress, OpenRequest,
};

use crate::background_work::{spawn_work, UiSender};
use crate::feedback_text::source_text;
use crate::request_tickets::{RequestKind, Ticket};
use crate::source_menu::SourceFilter;
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_feedback::show_feedback;
use super::player_polling::{clear_video_frame, reset_tracks_ui, sync_subtitles_indicator};
use super::source_picker::show_sources;
use super::subtitle_delivery::{DeliveryRequest, SubtitleDeliveryHandle};

/// What a worker needs to open or refresh one video.
pub(crate) struct OpenJob {
    pub(crate) ticket: Ticket,
    pub(crate) services: UiServices,
    pub(crate) item: PlaybackItem,
    pub(crate) data: String,
    pub(crate) control: OpenControl,
    pub(crate) live: LiveSources,
}

/// Starts finding streams for `item` (`data` is the provider's episode or movie payload);
/// `shown_title` is what the player shows (the item keeps the show's own title for the history).
pub(crate) fn begin_open(ctx: &BridgeContext, item: PlaybackItem, data: String, shown_title: &str) {
    let Some(playback) = &ctx.services.playback else {
        let problem = ctx.services.player_problem.clone();
        ctx.sender()
            .notice(problem.unwrap_or_else(|| "no player".into()));
        return;
    };
    apply_watching_cache(ctx, playback);
    claim_title_caches(ctx, &item);
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
        session.watch.reset();
        session.source_filter = SourceFilter::default();
        session.reset_for_new_open();
        let delivery = start_subtitle_delivery(ctx, &item, ticket, HashSet::new());
        session.subtitle_delivery = Some(delivery);
        (session.control.clone(), session.live.clone(), ticket)
    };
    if let Some(ui) = ctx.window() {
        clear_video_frame(ctx, &ui);
        show_opening_started(&ui, shown_title);
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

/// Drops other titles' source/subtitle files; remembers the keep token for leave-save.
fn claim_title_caches(ctx: &BridgeContext, item: &PlaybackItem) {
    let key = title_key(&item.provider, &item.media_key);
    let kept = ctx.services.title_cache.keep_only(&key);
    ctx.services.subtitles.keep_only_title(&key);
    ctx.session.borrow_mut().kept_sources = Some(kept);
}

/// Starts background subtitle downloading/caching for `item` under `ticket`.
fn start_subtitle_delivery(
    ctx: &BridgeContext,
    item: &PlaybackItem,
    ticket: Ticket,
    already_delivered: HashSet<String>,
) -> SubtitleDeliveryHandle {
    let preferred = ctx
        .services
        .settings
        .preferred_subtitles()
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "cannot read preferred subtitle languages");
            Vec::new()
        });
    SubtitleDeliveryHandle::start(DeliveryRequest {
        loader: Arc::clone(&ctx.services.subtitle_loader),
        title_key: title_key(&item.provider, &item.media_key),
        preferred,
        already_delivered,
        ticket,
        sender: ctx.sender(),
    })
}

/// The opening panel starts empty: nothing found, nothing to play yet.
fn show_opening_started(ui: &CloudStream, shown_title: &str) {
    ui.set_open_busy(true);
    ui.set_open_title("Finding sources…".into());
    ui.set_open_status("".into());
    ui.set_open_fraction(0.0);
    ui.set_can_skip_checks(false);
    ui.set_sources_loading(true);
    ui.set_sources_status("Finding sources…".into());
    ui.set_subtitles_loading(true);
    ui.set_subtitles_status("Searching for subtitles…".into());
    ui.set_focus_sources(false);
    ui.set_focus_subtitles(false);
    ui.set_player_title(shown_title.into());
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
    sender.send(UiEvent::OpenFinished(ticket));
}

/// New ticket, control, and subtitle delivery handle so the still-running open cannot race refresh.
pub(crate) fn build_refresh_job(
    ctx: &BridgeContext,
) -> (OpenJob, Option<core_model::PlayableLink>) {
    let mut session = ctx.session.borrow_mut();
    let (control, ticket) = session.begin_source_refresh();
    let playing_link = session.links.get(session.selected).cloned();
    let item = session
        .target
        .clone()
        .unwrap_or_else(|| empty_item(&session.details.provider));
    let delivery =
        start_subtitle_delivery(ctx, &item, ticket, session.delivered_subtitle_urls.clone());
    if !session.provider_subtitles.is_empty() {
        delivery.submit(&session.provider_subtitles);
    }
    session.subtitle_delivery = Some(delivery);
    let job = OpenJob {
        ticket,
        services: ctx.owned_services(),
        item,
        data: session.episode_data.clone(),
        control,
        live: session.live.clone(),
    };
    (job, playing_link)
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
        (item, link, session.ready_subtitles.clone())
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
    let start_ms = resume_position(ctx);
    if let Err(error) = play_stream(ctx, index, start_ms) {
        tracing::warn!(%error, index, "the picked stream did not start");
        ui.set_notice(format!("Cannot start playback: {error}").into());
    }
}

/// Where the target was left off (0 when it was finished or never started).
fn resume_position(ctx: &BridgeContext) -> i64 {
    let (Some(playback), Some(item)) =
        (&ctx.services.playback, ctx.session.borrow().target.clone())
    else {
        return 0;
    };
    playback.resume_position_ms(&item).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read the saved position; starting from the beginning");
        0
    })
}

/// Applies the Settings demuxer budget for active playback.
fn apply_watching_cache(ctx: &BridgeContext, playback: &app_services::PlaybackService) {
    let cache = ctx.services.settings.video_cache().unwrap_or_default();
    if let Err(error) = playback.set_cache_limits(cache.while_watching()) {
        tracing::warn!(%error, "the video cache size could not be given to the player");
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
            reset_tracks_ui(ctx, &ui);
            sync_subtitles_indicator(ctx, &ui);
            show_sources(ctx);
            let label = ctx.session.borrow().links.get(index).map(full_label);
            if let Some(label) = label {
                show_feedback(&ui, "source", source_text(&label), None);
            }
        }
        Err(error) => {
            tracing::warn!(%error, index, "switching the source failed");
            ui.set_notice(format!("Cannot switch source: {error}").into());
        }
    }
}
