//! Search and Browse: run them and open their results.

use slint::Model;

use crate::background_work::spawn_work;
use crate::request_tickets::RequestKind;
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::browse_loading::{load_browse, open_browse_title};
use super::details_loading::load_details;
use super::home_callbacks::row;
use super::title_search::{enabled_providers, search_providers};

pub(crate) fn wire_catalog(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_search(move |query| search_titles(&c, query.trim().to_owned()));
    let c = ctx.clone();
    ui.on_open_search_hit(move |index| {
        let item = c
            .window()
            .and_then(|ui| ui.get_search_items().row_data(row(index)));
        if let Some(item) = item {
            load_details(&c, &item);
        }
    });
    let c = ctx.clone();
    ui.on_open_browse_hit(move |index| open_browse_title(&c, row(index)));
    let c = ctx.clone();
    ui.on_load_browse(move |kind| load_browse(&c, kind.to_string()));
}

/// Browse loads the first time its tab is shown (and again after a failure).
pub(crate) fn load_browse_if_empty(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    if ui.get_browse_items().row_count() == 0 && !ui.get_browse_loading() {
        load_browse(ctx, ui.get_browse_kind().to_string());
    }
}

fn search_titles(ctx: &BridgeContext, query: String) {
    let Some(ui) = ctx.window() else {
        return;
    };
    if query.is_empty() {
        return;
    }
    ui.set_search_loading(true);
    ui.set_search_message("".into());
    let preferred = ui.get_selected_provider().to_string();
    let ticket = ctx.issue_ticket(RequestKind::Search);
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("search", move || {
        let providers = enabled_providers(&services, &preferred);
        let summary = search_providers(&services, &providers, &query);
        sender.send(UiEvent::SearchLoaded(ticket, summary));
    });
}
