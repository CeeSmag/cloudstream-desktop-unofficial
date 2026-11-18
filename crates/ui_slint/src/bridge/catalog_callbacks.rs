//! Search and Browse: run them and open their results.

use slint::Model;

use crate::background_work::spawn_work;
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::browse_loading::load_browse;
use super::details_loading::load_details;
use super::home_callbacks::row;

pub(crate) fn wire_catalog(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_search(move |query| search_titles(&c, query.to_string()));
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
    ui.on_open_browse_hit(move |index| {
        let item = c
            .window()
            .and_then(|ui| ui.get_browse_items().row_data(row(index)));
        if let Some(item) = item {
            load_details(&c, &item);
        }
    });
    let c = ctx.clone();
    ui.on_load_browse(move |kind| load_browse(&c, kind.to_string()));
}

fn search_titles(ctx: &BridgeContext, query: String) {
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("search", move || {
        let providers = services.extensions.providers();
        let mut items = Vec::new();
        for outcome in services.catalog.search(&providers, &query) {
            match outcome.result {
                Ok(page) => items.extend(page.items),
                Err(error) => tracing::warn!(%error, "a provider's search failed"),
            }
        }
        sender.send(UiEvent::SearchLoaded(items));
    });
}
