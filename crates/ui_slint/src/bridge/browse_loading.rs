//! The Browse screen: popular TMDB titles, and opening one on the viewer's providers.
//!
//! Browse lists TMDB titles, which no provider can load by their TMDB id (the old screen passed
//! that id to the provider as if it were the provider's address, so every title failed to open).
//! Opening one searches the enabled providers for its name and opens the best match.

use app_services::{pick_best_search_match, BrowseService};

use crate::background_work::spawn_work;
use crate::posters::poster_from_search;
use crate::request_tickets::RequestKind;
use crate::ui_events::{BrowseMatch, UiEvent};

use super::bridge_context::BridgeContext;
use super::details_loading::{load_details, show_details_failed, show_details_pending};
use super::title_search::{enabled_providers, search_providers};

/// Shows `kind` ("movie" or "tv") with the current filters, from the first page.
pub(crate) fn load_browse(ctx: &BridgeContext, kind: String) {
    ctx.session.borrow_mut().browse_filters.kind = kind;
    fetch_browse(ctx, 1);
}

/// Fetches page `page` of Browse with the viewer's filters; page 1 replaces the grid, later
/// pages are added to it.
pub(crate) fn fetch_browse(ctx: &BridgeContext, page: u32) {
    let request = ctx.session.borrow().browse_filters.request(page);
    if let Some(ui) = ctx.window() {
        ui.set_browse_kind(request.media_type.clone().into());
        ui.set_browse_loading(true);
        ui.set_browse_message("".into());
    }
    let ticket = ctx.issue_ticket(RequestKind::Browse);
    let sender = ctx.sender();
    spawn_work("browse", move || {
        let result = BrowseService::default()
            .fetch_browse_page(&request)
            .map_err(|error| error.to_string());
        if let Err(error) = &result {
            tracing::warn!(kind = %request.media_type, page, %error, "Browse could not load");
        }
        sender.send(UiEvent::BrowseLoaded(ticket, result));
    });
}

pub(crate) fn open_browse_title(ctx: &BridgeContext, index: usize) {
    let Some(item) = ctx.session.borrow().browse_items.get(index).cloned() else {
        return;
    };
    let ticket = ctx.issue_ticket(RequestKind::Details);
    let preferred = ctx
        .window()
        .map(|ui| ui.get_selected_provider().to_string())
        .unwrap_or_default();
    show_details_pending(
        ctx,
        &item.name,
        &format!("Finding “{}” on your providers…", item.name),
    );
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("browse-match", move || {
        let providers = enabled_providers(&services, &preferred);
        let summary = search_providers(&services, &providers, &item.name);
        let found = pick_best_search_match(&item, &summary.items).cloned();
        tracing::info!(
            found = found.is_some(),
            asked = summary.asked,
            "matched a Browse title"
        );
        let title = item.name;
        sender.send(UiEvent::BrowseMatched(ticket, BrowseMatch { title, found }));
    });
}

/// Opens the matched title's page, or says it was not found.
pub(crate) fn show_browse_match(ctx: &BridgeContext, matched: BrowseMatch) {
    match matched.found {
        Some(result) => load_details(ctx, &poster_from_search(&result)),
        None => show_details_failed(
            ctx,
            &format!("“{}” was not found on your providers.", matched.title),
        ),
    }
}
