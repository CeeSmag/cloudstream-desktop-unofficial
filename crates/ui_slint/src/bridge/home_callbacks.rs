//! Tabs, notices, Home shelves, the hero and continue-watching.

use slint::Model;

use crate::home_scroll::{is_home_on_screen, is_home_top_request, reset_home_scroll};
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::catalog_callbacks::load_browse_if_empty;
use super::catalog_events::show_library;
use super::details_callbacks::close_details;
use super::details_loading::load_details;
use super::focus_navigation::restore_focus;
use super::home_expanded::close_expanded_shelf;
use super::storage_usage_view::measure_storage_usage;

/// The Browse tab's index in the top bar.
const BROWSE_TAB: i32 = 1;
/// The Library tab's index in the top bar.
const LIBRARY_TAB: i32 = 3;
const SETTINGS_TAB: i32 = 4;

pub(crate) fn wire_home(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_select_tab(move |tab| select_tab(&c, tab));
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
            if ui.get_hero_index() >= 0 {
                ui.invoke_open_library_hit(ui.get_hero_index());
            }
        }
    });
}

fn select_tab(ctx: &BridgeContext, tab: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let home_on_screen = is_home_on_screen(ui.get_tab(), !ui.get_details_title().is_empty());
    tracing::debug!(
        from = ui.get_tab(),
        to = tab,
        home_on_screen,
        "ui tab selected"
    );
    if !ui.get_details_title().is_empty() {
        close_details(ctx);
    }
    ui.set_tab(tab);
    if is_home_top_request(home_on_screen, tab) {
        // Coming back to Home from elsewhere keeps its place; Home pressed on Home starts over.
        close_expanded_shelf(ctx);
        reset_home_scroll(&ui);
    }
    if tab == BROWSE_TAB {
        load_browse_if_empty(ctx);
    }
    if tab == LIBRARY_TAB {
        show_library(ctx, &ui);
    }
    if tab == SETTINGS_TAB {
        measure_storage_usage(ctx);
    }
    restore_focus(ctx, &ui);
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
        tracing::debug!(shelf, index, title = %item.title, provider = %item.provider, "opening home shelf item");
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

/// A Slint row index as a model index (negative indexes never match a row).
pub(crate) fn row(index: i32) -> usize {
    usize::try_from(index).unwrap_or(usize::MAX)
}
