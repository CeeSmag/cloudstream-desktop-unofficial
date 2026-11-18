//! Background poster artwork hydration for Slint models.

use slint::{ComponentHandle, Model, ModelRc};

use crate::poster_images::{fetch_poster_buffer, image_from_buffer, PosterImageCache};
use crate::runner::{spawn_work, PosterKind};
use crate::{CloudStream, PosterItem};

pub(crate) fn hydrate_poster_images(
    ui: &slint::Weak<CloudStream>,
    cache: &PosterImageCache,
    kind: PosterKind,
) {
    let Some(ui_h) = ui.upgrade() else {
        return;
    };
    let urls: Vec<(usize, usize, String)> = match kind {
        PosterKind::Continue => collect_urls(&ui_h.get_continue_items()),
        PosterKind::Search => collect_urls(&ui_h.get_search_items()),
        PosterKind::Browse => collect_urls(&ui_h.get_browse_items()),
        PosterKind::Library => collect_urls(&ui_h.get_library_items()),
        PosterKind::Home => collect_urls(&ui_h.get_home_items()),
        PosterKind::Shelf => {
            let mut out = Vec::new();
            let shelves = ui_h.get_home_shelves();
            for shelf in 0..shelves.row_count() {
                if let Some(section) = shelves.row_data(shelf) {
                    for (index, _, url) in collect_urls(&section.items) {
                        out.push((shelf, index, url));
                    }
                }
            }
            out
        }
    };
    for (shelf_or_zero, index, url) in urls {
        if url.is_empty() {
            continue;
        }
        if let Some(buffer) = cache.get(&url) {
            if let Some(ui_h) = ui.upgrade() {
                apply_image_on(&ui_h, kind, shelf_or_zero, index, image_from_buffer(buffer));
            }
            continue;
        }
        let ui = ui.clone();
        let cache = cache.clone();
        spawn_work("poster-image", move || {
            if let Some(buffer) = fetch_poster_buffer(&url) {
                cache.insert(url, buffer.clone());
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    apply_image_on(&ui, kind, shelf_or_zero, index, image_from_buffer(buffer));
                });
            }
        });
    }
}

fn collect_urls(model: &ModelRc<PosterItem>) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    for index in 0..model.row_count() {
        if let Some(item) = model.row_data(index) {
            let url = item.poster.to_string();
            if !url.is_empty() {
                out.push((0, index, url));
            }
        }
    }
    out
}

fn apply_image_on(
    ui: &CloudStream,
    kind: PosterKind,
    shelf: usize,
    index: usize,
    image: slint::Image,
) {
    match kind {
        PosterKind::Continue => {
            set_row_image(&ui.get_continue_items(), ui, "continue", index, image)
        }
        PosterKind::Search => set_row_image(&ui.get_search_items(), ui, "search", index, image),
        PosterKind::Browse => set_row_image(&ui.get_browse_items(), ui, "browse", index, image),
        PosterKind::Library => set_row_image(&ui.get_library_items(), ui, "library", index, image),
        PosterKind::Home => set_row_image(&ui.get_home_items(), ui, "home", index, image),
        PosterKind::Shelf => {
            let shelves = ui.get_home_shelves();
            if let Some(section) = shelves.row_data(shelf) {
                if let Some(mut item) = section.items.row_data(index) {
                    item.image = image;
                    section.items.set_row_data(index, item);
                    shelves.set_row_data(shelf, section);
                    ui.set_home_shelves(shelves);
                }
            }
        }
    }
}

fn set_row_image(
    model: &ModelRc<PosterItem>,
    ui: &CloudStream,
    which: &str,
    index: usize,
    image: slint::Image,
) {
    if let Some(mut item) = model.row_data(index) {
        item.image = image;
        model.set_row_data(index, item);
        match which {
            "continue" => ui.set_continue_items(model.clone()),
            "search" => ui.set_search_items(model.clone()),
            "browse" => ui.set_browse_items(model.clone()),
            "library" => ui.set_library_items(model.clone()),
            "home" => ui.set_home_items(model.clone()),
            _ => {}
        }
    }
}
