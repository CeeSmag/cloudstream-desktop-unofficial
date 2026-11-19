//! Opens a video: search and check its streams, play one, refresh or switch the source.

use std::sync::atomic::{AtomicBool, Ordering};

use app_services::{full_label, PlaybackItem, StreamSelection, UiServices};
use playback_session::{
    run_open, title_key, CheckedOpen, LiveSources, OpenControl, OpenProgress, OpenRequest,
    RefreshRequest,
};

use crate::background_work::{spawn_work, UiSender};
use crate::feedback_text::source_text;
use crate::request_tickets::{RequestKind, Ticket};
use crate::source_menu::SourceFilter;
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_feedback::show_feedback;
use super::player_polling::clear_video_frame;
use super::source_picker::show_sources;

/// What a worker needs to open one video.
struct OpenJob {
    ticket: Ticket,
    services: UiServices,
    item: PlaybackItem,
    data: String,
    control: OpenControl,
    live: LiveSources,
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

/// The opening panel starts empty: nothing found, nothing to play yet.
fn show_opening_started(ui: &CloudStream, shown_title: &str) {
    ui.set_open_busy(true);
    ui.set_open_title("Finding sources…".into());
    ui.set_open_status("".into());
    ui.set_open_fraction(0.0);
    ui.set_can_skip_checks(false);
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
}

/// Searches the playing title's streams again while the current one keeps playing.
pub(crate) fn refresh_playing(ctx: &BridgeContext) {
    let (job, playing_link) = prepare_refresh(ctx);
    let sender = ctx.sender();
    spawn_work("refresh-sources", move || {
        run_refresh_job(&job, playing_link, &sender)
    });
}

/// New ticket and control so the still-running open cannot race this refresh.
fn prepare_refresh(ctx: &BridgeContext) -> (OpenJob, Option<core_model::PlayableLink>) {
    let mut session = ctx.session.borrow_mut();
    let (control, ticket) = session.begin_source_refresh();
    let playing_link = session.links.get(session.selected).cloned();
    let job = OpenJob {
        ticket,
        services: ctx.owned_services(),
        item: session
            .target
            .clone()
            .unwrap_or_else(|| empty_item(&session.details.provider)),
        data: session.episode_data.clone(),
        control,
        live: session.live.clone(),
    };
    (job, playing_link)
}

fn run_refresh_job(
    job: &OpenJob,
    playing_link: Option<core_model::PlayableLink>,
    sender: &UiSender,
) {
    let ticket = job.ticket;
    let on_progress = |progress: OpenProgress| sender.send(UiEvent::OpenProgress(ticket, progress));
    let on_update = |open: &CheckedOpen| sender.send(UiEvent::OpenUpdate(ticket, open.clone()));
    let request = RefreshRequest {
        streams: &job.services.streams,
        provider: &job.item.provider,
        data: &job.data,
        media_key: &job.item.media_key,
        playing_link: playing_link.as_ref(),
        cache: Some(&job.services.title_cache),
        live: &job.live,
        control: &job.control,
        on_progress: &on_progress,
        on_update: &on_update,
    };
    match playback_session::refresh_sources(request) {
        Ok(opened) => sender.send(UiEvent::OpenUpdate(ticket, opened)),
        Err(error) => sender.notice(format!("Refresh failed: {error}")),
    }
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
