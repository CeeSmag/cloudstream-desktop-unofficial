//! Shows background results: one handler per kind of event.

use crate::ui_events::{UiEvent, UpdateViewState};
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::browse_loading::show_browse_match;
use super::catalog_events::{
    show_browse, show_details, show_home, show_search, show_startup_done, show_startup_progress,
};
use super::details_loading::show_details_failed;
use super::extension_catalog::{load_extensions_catalog, show_catalog};
use super::extension_changes::{show_change_done, show_security_blocked};
use super::home_expanded::show_home_section_page;
use super::player_timeline::show_online_parts;
use super::repository_discovery::show_discovered_repositories;
use super::storage_usage_view::show_storage_usage;
use super::stream_events::{
    on_open_finished, on_subtitle_ready, show_open_progress, show_open_update, start_ready_stream,
};
use super::webview_dialog::show_webview_event;

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
        UiEvent::HomeSectionPageLoaded(_, page) => show_home_section_page(ctx, ui, page),
        UiEvent::SearchLoaded(_, summary) => show_search(ctx, ui, &summary),
        UiEvent::BrowseLoaded(_, result) => show_browse(ctx, ui, result),
        UiEvent::BrowseMatched(_, matched) => show_browse_match(ctx, matched),
        UiEvent::DetailsLoaded(_, page) => show_details(ctx, ui, *page),
        UiEvent::WebView(event) => show_webview_event(ctx, ui, event),
        UiEvent::DetailsFailed(_, error) => {
            show_details_failed(ctx, &format!("The title could not be loaded: {error}"));
        }
        other => apply_player_or_system_event(ctx, ui, other),
    }
}

fn apply_player_or_system_event(ctx: &BridgeContext, ui: &CloudStream, event: UiEvent) {
    match event {
        UiEvent::OpenProgress(_, progress) => show_open_progress(ctx, ui, progress),
        UiEvent::OpenReady(_, index) => start_ready_stream(ctx, ui, index),
        UiEvent::OpenUpdate(_, open) => show_open_update(ctx, ui, open),
        UiEvent::OpenFinished(_) => on_open_finished(ctx, ui),
        UiEvent::SubtitleReady(_, ready) => on_subtitle_ready(ctx, ui, ready),
        UiEvent::SkipTimesFound(_, parts) => show_online_parts(ctx, ui, parts),
        UiEvent::OpenFailed(_, error) => {
            on_open_finished(ctx, ui);
            ui.set_open_busy(false);
            ui.set_notice(error.into());
        }
        UiEvent::Notice(text) => ui.set_notice(text.into()),
        UiEvent::UpdateState(state) => apply_update_state(ui, state),
        UiEvent::UpdateDownloadProgress { label, percent } => {
            ui.set_update_label(label.into());
            ui.set_update_progress(f32::from(percent) / 100.0);
        }
        UiEvent::StorageMeasured(usage) => show_storage_usage(ctx, ui, &usage),
        UiEvent::ExtensionChanged {
            message,
            watch_repositories,
        } => show_change_done(ctx, ui, message, watch_repositories),
        UiEvent::ExtensionSecurityBlocked { hit, block } => {
            show_security_blocked(ctx, ui, hit.map(|h| *h), *block);
        }
        UiEvent::ExtensionsLoaded(catalog) => show_catalog(ctx, ui, *catalog),
        UiEvent::RepositoriesDiscovered {
            added,
            repositories,
        } => show_discovered_repositories(ctx, ui, added, repositories),
        UiEvent::ReloadExtensionsCatalog => load_extensions_catalog(ctx),
        _ => {}
    }
}

fn apply_update_state(ui: &CloudStream, state: UpdateViewState) {
    ui.set_update_label(state.label.into());
    ui.set_update_badge(state.badge.into());
    ui.set_update_version(state.version.into());
    ui.set_update_notes(state.notes.into());
    ui.set_update_can_apply(state.can_apply);
    ui.set_update_busy(state.busy);
    ui.set_update_downloading(state.downloading);
    ui.set_update_progress(f32::from(state.progress_percent) / 100.0);
}
