//! Taking titles out of the Library and out of Continue watching.

use slint::Model;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::catalog_events::{show_continue_watching, show_library};
use super::home_callbacks::row;

pub(crate) fn wire_library_actions(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_remove_continue(move |index| remove_continue(&c, index));
    let c = ctx.clone();
    ui.on_clear_continue(move || clear_continue(&c));
    let c = ctx.clone();
    ui.on_remove_bookmark(move |index| remove_bookmark(&c, index));
}

/// Forgets where the viewer was in one title of Continue watching.
pub(crate) fn remove_continue(ctx: &BridgeContext, index: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let Some(item) = ui.get_continue_items().row_data(row(index)) else {
        return;
    };
    match ctx
        .services
        .library
        .remove_continue_watching(&item.provider, &item.url)
    {
        Ok(_) => ui.set_notice(format!("Removed “{}” from Continue watching", item.title).into()),
        Err(error) => {
            tracing::warn!(%error, "could not remove a continue-watching entry");
            ui.set_notice("Could not remove it from Continue watching".into());
        }
    }
    show_continue_watching(ctx, &ui);
}

/// Empties Continue watching (the viewer confirmed it).
fn clear_continue(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    match ctx.services.library.clear_continue_watching() {
        Ok(removed) => {
            tracing::info!(titles = removed.len(), "continue watching cleared");
            ui.set_notice("Continue watching cleared".into());
        }
        Err(error) => {
            tracing::warn!(%error, "could not clear continue watching");
            ui.set_notice("Could not clear Continue watching".into());
        }
    }
    show_continue_watching(ctx, &ui);
}

/// Takes the Library's title at `index` out of it.
pub(crate) fn remove_bookmark(ctx: &BridgeContext, index: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let Some(item) = ui.get_library_items().row_data(row(index)) else {
        return;
    };
    match ctx
        .services
        .library
        .remove_bookmark(&item.provider, &item.url)
    {
        Ok(()) => ui.set_notice(format!("Removed “{}” from the Library", item.title).into()),
        Err(error) => {
            tracing::warn!(%error, "could not remove a bookmark");
            ui.set_notice("Could not remove it from the Library".into());
        }
    }
    show_library(ctx, &ui);
}
