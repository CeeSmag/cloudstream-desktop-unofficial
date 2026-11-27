//! Refreshes the playing title's streams while the current stream continues playing.

use playback_session::{CheckedOpen, OpenProgress, RefreshRequest};

use crate::background_work::{spawn_work, UiSender};
use crate::ui_events::UiEvent;

use super::bridge_context::BridgeContext;
use super::stream_opening::{build_refresh_job, OpenJob};

/// Searches the playing title's streams again while the current one keeps playing.
pub(crate) fn refresh_playing(ctx: &BridgeContext) {
    let (job, playing_link) = build_refresh_job(ctx);
    if let Some(ui) = ctx.window() {
        ui.set_sources_loading(true);
        ui.set_sources_status("Refreshing sources…".into());
        ui.set_focus_sources(false);
        ui.set_focus_subtitles(false);
    }
    let sender = ctx.sender();
    spawn_work("refresh-sources", move || {
        run_refresh_job(&job, playing_link, &sender);
    });
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
    sender.send(UiEvent::OpenFinished(ticket));
}
