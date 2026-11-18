//! Shows background results: one handler per kind of event.

use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::browse_loading::show_browse_match;
use super::catalog_events::{
    show_browse, show_details, show_home, show_search, show_startup_done, show_startup_progress,
};
use super::details_loading::show_details_failed;
use super::stream_events::{show_open_progress, show_open_update, start_ready_stream};

/// Shows everything the workers finished since the last tick.
pub(crate) fn drain_events(ctx: &BridgeContext, ui: &CloudStream) {
    for event in ctx.pump.drain() {
        apply_event(ctx, ui, event);
    }
}

fn apply_event(ctx: &BridgeContext, ui: &CloudStream, event: UiEvent) {
    if let Some(ticket) = event.ticket() {
        if !ctx.session.borrow().tickets.is_current(ticket) {
            tracing::debug!(kind = ?ticket.kind(), "dropping the answer to a replaced request");
            return;
        }
    }
    match event {
        UiEvent::StartupProgress {
            finished,
            total,
            current,
        } => show_startup_progress(ui, finished, total, &current),
        UiEvent::StartupDone { providers, error } => {
            show_startup_done(ctx, ui, &providers, error);
        }
        UiEvent::HomeLoaded(_, sections) => show_home(ctx, ui, &sections),
        UiEvent::SearchLoaded(_, summary) => show_search(ctx, ui, &summary),
        UiEvent::BrowseLoaded(_, result) => show_browse(ctx, ui, result),
        UiEvent::BrowseMatched(_, matched) => show_browse_match(ctx, matched),
        UiEvent::DetailsLoaded(_, page) => show_details(ctx, ui, *page),
        UiEvent::OpenProgress(_, progress) => show_open_progress(ui, progress),
        UiEvent::OpenReady(_, index) => start_ready_stream(ctx, ui, index),
        UiEvent::OpenUpdate(_, open) => show_open_update(ctx, ui, open),
        UiEvent::OpenFailed(_, error) => {
            ui.set_open_busy(false);
            ui.set_notice(error.into());
        }
        UiEvent::DetailsFailed(_, error) => {
            show_details_failed(ctx, &format!("The title could not be loaded: {error}"));
        }
        UiEvent::Notice(text) => ui.set_notice(text.into()),
        UiEvent::UpdateLabel(label) => ui.set_update_label(label.into()),
    }
}
