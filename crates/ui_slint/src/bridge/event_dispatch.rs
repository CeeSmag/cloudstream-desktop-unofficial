//! Shows background results: one handler per kind of event.

use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::catalog_events::{
    show_browse, show_details, show_home, show_search, show_startup_done, show_startup_progress,
};
use super::stream_events::{show_open_progress, show_open_update, start_ready_stream};

/// Shows everything the workers finished since the last tick.
pub(crate) fn drain_events(ctx: &BridgeContext, ui: &CloudStream) {
    for event in ctx.pump.drain() {
        apply_event(ctx, ui, event);
    }
}

fn apply_event(ctx: &BridgeContext, ui: &CloudStream, event: UiEvent) {
    match event {
        UiEvent::StartupProgress {
            finished,
            total,
            current,
        } => show_startup_progress(ui, finished, total, &current),
        UiEvent::StartupDone { providers, error } => {
            show_startup_done(ctx, ui, &providers, error);
        }
        UiEvent::HomeLoaded(sections) => show_home(ctx, ui, &sections),
        UiEvent::SearchLoaded(items) => show_search(ctx, ui, &items),
        UiEvent::BrowseLoaded(rows) => show_browse(ctx, ui, rows),
        UiEvent::DetailsLoaded(page) => show_details(ctx, ui, *page),
        UiEvent::OpenProgress(progress) => show_open_progress(ui, progress),
        UiEvent::OpenReady { index } => start_ready_stream(ctx, ui, index),
        UiEvent::OpenUpdate(open) => show_open_update(ctx, ui, open),
        UiEvent::OpenFailed(error) => {
            ui.set_open_busy(false);
            ui.set_notice(error.into());
        }
        UiEvent::DetailsFailed(text) | UiEvent::Notice(text) => ui.set_notice(text.into()),
        UiEvent::CloudflareNeeded { host, url, id } => {
            {
                let mut session = ctx.session.borrow_mut();
                session.cf_host = Some(host.clone());
                session.cf_id = Some(id);
            }
            ui.set_cf_host(host.into());
            ui.set_cf_url(url.into());
            ui.set_show_cf(true);
        }
        UiEvent::UpdateLabel(label) => ui.set_update_label(label.into()),
    }
}
