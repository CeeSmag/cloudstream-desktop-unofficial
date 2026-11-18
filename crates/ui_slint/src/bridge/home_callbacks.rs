//! Tabs, notices, Home shelves, the hero and continue-watching.

use slint::Model;

use crate::posters::posters_from_continue;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::catalog_callbacks::load_browse_if_empty;
use super::details_loading::load_details;

/// The Browse tab's index in the top bar.
const BROWSE_TAB: i32 = 1;

pub(crate) fn wire_home(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_select_tab(move |tab| {
        if let Some(ui) = c.window() {
            ui.set_tab(tab);
        }
        if tab == BROWSE_TAB {
            load_browse_if_empty(&c);
        }
    });
    let weak = ctx.ui.clone();
    ui.on_dismiss_notice(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_notice("".into());
        }
    });
    let c = ctx.clone();
    ui.on_open_shelf_item(move |shelf, index| open_shelf_item(&c, shelf, index));
    let c = ctx.clone();
    ui.on_open_library_hit(move |index| open_library_hit(&c, index));
    let weak = ctx.ui.clone();
    ui.on_resume_hero(move || {
        if let Some(ui) = weak.upgrade() {
            if ui.get_continue_items().row_count() > 0 {
                ui.invoke_open_library_hit(0);
            }
        }
    });
    let weak = ctx.ui.clone();
    ui.on_play_continue(move |index| {
        if let Some(ui) = weak.upgrade() {
            ui.invoke_open_library_hit(index);
        }
    });
    let c = ctx.clone();
    ui.on_remove_continue(move |index| remove_continue(&c, index));
}

fn open_shelf_item(ctx: &BridgeContext, shelf: i32, index: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let item = ui
        .get_home_shelves()
        .row_data(row(shelf))
        .and_then(|section| section.items.row_data(row(index)));
    if let Some(item) = item {
        load_details(ctx, &item);
    }
}

/// Opens a title of the continue-watching row (on Home) or of the Library.
fn open_library_hit(ctx: &BridgeContext, index: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let model = if ui.get_tab() == 0 {
        ui.get_continue_items()
    } else {
        ui.get_library_items()
    };
    if let Some(item) = model.row_data(row(index)) {
        load_details(ctx, &item);
    }
}

fn remove_continue(ctx: &BridgeContext, index: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let Some(item) = ui.get_continue_items().row_data(row(index)) else {
        return;
    };
    let library = &ctx.services.library;
    if let Err(error) = library.remove_continue_watching(&item.provider, &item.url) {
        tracing::warn!(%error, "could not remove a continue-watching entry");
        ui.set_notice("Could not remove it from Continue watching".into());
    }
    match library.continue_watching() {
        Ok(list) => ui.set_continue_items(posters_from_continue(&list)),
        Err(error) => tracing::warn!(%error, "could not read the watch history"),
    }
}

/// A Slint row index as a model index (negative indexes never match a row).
pub(crate) fn row(index: i32) -> usize {
    usize::try_from(index).unwrap_or(usize::MAX)
}
