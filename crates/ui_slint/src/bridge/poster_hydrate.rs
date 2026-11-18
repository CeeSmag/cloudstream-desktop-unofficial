//! Fills poster models with their artwork in the background.

use slint::{Model, ModelRc};

use crate::background_work::spawn_work;
use crate::poster_images::{fetch_poster_buffer, image_from_buffer};
use crate::ui_events::PosterKind;
use crate::{CloudStream, PosterItem};

use super::bridge_context::BridgeContext;

/// Where one poster sits: the shelf (0 outside Home) and its row.
#[derive(Debug, Clone, Copy)]
struct PosterSlot {
    kind: PosterKind,
    shelf: usize,
    index: usize,
}

/// Starts loading the artwork of every poster in the `kind` model.
pub(crate) fn hydrate_poster_images(ctx: &BridgeContext, kind: PosterKind) {
    let Some(ui) = ctx.window() else {
        return;
    };
    for (shelf, index, url) in poster_urls(&ui, kind) {
        let slot = PosterSlot { kind, shelf, index };
        if let Some(buffer) = ctx.posters.get(&url) {
            apply_image(&ui, slot, image_from_buffer(buffer));
            continue;
        }
        let weak = ctx.ui.clone();
        let cache = ctx.posters.clone();
        spawn_work("poster-image", move || {
            if let Some(buffer) = fetch_poster_buffer(&url) {
                cache.insert(url, buffer.clone());
                let shown = weak.upgrade_in_event_loop(move |ui| {
                    apply_image(&ui, slot, image_from_buffer(buffer));
                });
                if let Err(error) = shown {
                    tracing::debug!(%error, "window closed before a poster arrived");
                }
            }
        });
    }
}

fn poster_urls(ui: &CloudStream, kind: PosterKind) -> Vec<(usize, usize, String)> {
    match kind {
        PosterKind::Continue => row_urls(0, &ui.get_continue_items()),
        PosterKind::Search => row_urls(0, &ui.get_search_items()),
        PosterKind::Browse => row_urls(0, &ui.get_browse_items()),
        PosterKind::Library => row_urls(0, &ui.get_library_items()),
        PosterKind::Shelf => {
            let shelves = ui.get_home_shelves();
            (0..shelves.row_count())
                .filter_map(|shelf| shelves.row_data(shelf).map(|section| (shelf, section)))
                .flat_map(|(shelf, section)| row_urls(shelf, &section.items))
                .collect()
        }
    }
}

fn row_urls(shelf: usize, model: &ModelRc<PosterItem>) -> Vec<(usize, usize, String)> {
    model
        .iter()
        .enumerate()
        .map(|(index, item)| (shelf, index, item.poster.to_string()))
        .filter(|(_, _, url)| !url.is_empty())
        .collect()
}

fn apply_image(ui: &CloudStream, slot: PosterSlot, image: slint::Image) {
    let model = match slot.kind {
        PosterKind::Continue => ui.get_continue_items(),
        PosterKind::Search => ui.get_search_items(),
        PosterKind::Browse => ui.get_browse_items(),
        PosterKind::Library => ui.get_library_items(),
        PosterKind::Shelf => {
            let Some(section) = ui.get_home_shelves().row_data(slot.shelf) else {
                return;
            };
            section.items
        }
    };
    if let Some(mut item) = model.row_data(slot.index) {
        item.image = image;
        model.set_row_data(slot.index, item);
    }
}
