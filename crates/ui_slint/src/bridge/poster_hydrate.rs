//! Fills poster models with their artwork, loaded by the artwork service off the UI thread.

use std::collections::HashMap;

use poster_artwork::{ArtworkRequest, ArtworkSize, DecodedArtwork};
use slint::{ComponentHandle, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer};

use crate::posters::decode_poster_headers;
use crate::ui_events::PosterKind;
use crate::{CloudStream, HomeExpanded, PosterItem};

use super::bridge_context::BridgeContext;

/// Where one poster sits: the model, the shelf (0 outside Home), the row and its address.
#[derive(Debug, Clone)]
struct PosterSlot {
    kind: PosterKind,
    shelf: usize,
    index: usize,
    url: String,
}

/// Shows the artwork of every poster in the `kind` model: from the picture store when the address
/// is already there (shared with every other tile), else loaded at tile size off the UI thread.
/// The model's addresses are pinned for `kind`, so the pictures on screen are never evicted.
pub(crate) fn hydrate_poster_images(ctx: &BridgeContext, kind: PosterKind) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let rows = poster_rows(&ui, kind);
    ctx.images.with(|store| {
        store.set_pins(
            kind,
            rows.iter().map(|(_, _, item)| item.poster.to_string()),
        );
    });
    let referers = provider_referers(ctx);
    for (shelf, index, item) in rows {
        let slot = PosterSlot {
            kind,
            shelf,
            index,
            url: item.poster.to_string(),
        };
        if let Some(pixels) = ctx.images.with(|store| store.get(&slot.url)) {
            apply_image(&ui, &slot, pixels);
            continue;
        }
        let referer = referers.get(item.provider.as_str()).map(String::as_str);
        request_tile(ctx, slot, &item, referer);
    }
}

/// Lets go of every picture `kind` showed (its model was cleared); they stay only as long as the
/// budget allows, for a quick return.
pub(crate) fn release_poster_images(ctx: &BridgeContext, kind: PosterKind) {
    ctx.images
        .with(|store| store.set_pins(kind, std::iter::empty()));
}

fn request_tile(ctx: &BridgeContext, slot: PosterSlot, item: &PosterItem, referer: Option<&str>) {
    let headers = poster_request_headers(&item.poster_headers, referer);
    let weak = ctx.ui.clone();
    let images = ctx.images.clone();
    let url = slot.url.clone();
    let request = ArtworkRequest {
        raw_url: &url,
        size: ArtworkSize::Tile,
        headers,
    };
    ctx.services.artwork.request_sized(
        request,
        Box::new(move |result| {
            let Ok(artwork) = result else {
                return; // the loader logged why; the tile keeps its placeholder
            };
            let shown = weak.upgrade_in_event_loop(move |ui| {
                // Every tile of this address ends up with the same buffer, filed once.
                let pixels = images.with(|store| match store.get(&slot.url) {
                    Some(stored) => stored,
                    None => store.insert(&slot.url, pixel_buffer(&artwork)),
                });
                apply_image(&ui, &slot, pixels);
            });
            if let Err(error) = shown {
                tracing::debug!(%error, "window closed before a poster arrived");
            }
        }),
    );
}

/// The provider's own poster headers, plus its site as `Referer` when it named none.
fn poster_request_headers(encoded: &str, referer: Option<&str>) -> Vec<(String, String)> {
    let mut headers = decode_poster_headers(encoded);
    let has_referer = headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("referer"));
    if let (false, Some(referer)) = (has_referer, referer) {
        headers.push(("Referer".to_owned(), referer.to_owned()));
    }
    headers
}

/// Loads one image (a hero artwork) and hands it to `on_ready` on the UI thread.
pub(crate) fn request_artwork(
    ctx: &BridgeContext,
    url: &str,
    on_ready: impl FnOnce(&CloudStream, slint::Image) + Send + 'static,
) {
    if url.is_empty() {
        return;
    }
    let weak = ctx.ui.clone();
    let callback: poster_artwork::ArtworkCallback = Box::new(move |result| {
        let Ok(artwork) = result else {
            return;
        };
        let buffer = pixel_buffer(&artwork);
        let shown =
            weak.upgrade_in_event_loop(move |ui| on_ready(&ui, slint::Image::from_rgba8(buffer)));
        if shown.is_err() {
            tracing::debug!("window closed before an image arrived");
        }
    });
    ctx.services.artwork.request(url, Vec::new(), callback);
}

/// Details artwork: memory only (never written to `artwork/` on disk), with optional `Referer`.
pub(crate) fn request_ephemeral_with_referer(
    ctx: &BridgeContext,
    url: &str,
    referer: Option<&str>,
    on_ready: impl FnOnce(&CloudStream, slint::Image) + Send + 'static,
) {
    if url.is_empty() {
        return;
    }
    if let Some(artwork) = ctx.services.artwork.cached(url) {
        if let Some(ui) = ctx.window() {
            on_ready(&ui, slint::Image::from_rgba8(pixel_buffer(&artwork)));
        }
        return;
    }
    let headers = referer
        .filter(|value| !value.is_empty())
        .map(|value| vec![("Referer".to_owned(), value.to_owned())])
        .unwrap_or_default();
    let weak = ctx.ui.clone();
    let callback: poster_artwork::ArtworkCallback = Box::new(move |result| {
        let Ok(artwork) = result else {
            return;
        };
        let buffer = pixel_buffer(&artwork);
        let shown =
            weak.upgrade_in_event_loop(move |ui| on_ready(&ui, slint::Image::from_rgba8(buffer)));
        if shown.is_err() {
            tracing::debug!("window closed before an image arrived");
        }
    });
    ctx.services
        .artwork
        .request_ephemeral(url, headers, callback);
}

/// Site `Referer` for a single provider, if known.
pub(crate) fn provider_referer(ctx: &BridgeContext, provider: &str) -> Option<String> {
    provider_referers(ctx).remove(provider)
}

/// Each provider's site, sent as the Referer of its posters (some hosts refuse hot-linking).
fn provider_referers(ctx: &BridgeContext) -> HashMap<String, String> {
    ctx.services
        .extensions
        .providers()
        .into_iter()
        .filter(|info| !info.main_url.is_empty())
        .map(|info| {
            (
                info.name,
                format!("{}/", info.main_url.trim_end_matches('/')),
            )
        })
        .collect()
}

fn pixel_buffer(artwork: &DecodedArtwork) -> SharedPixelBuffer<Rgba8Pixel> {
    SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&artwork.rgba, artwork.width, artwork.height)
}

fn poster_rows(ui: &CloudStream, kind: PosterKind) -> Vec<(usize, usize, PosterItem)> {
    match kind {
        PosterKind::Continue => model_rows(0, &ui.get_continue_items()),
        PosterKind::Search => model_rows(0, &ui.get_search_items()),
        PosterKind::Browse => model_rows(0, &ui.get_browse_items()),
        PosterKind::Library => model_rows(0, &ui.get_library_items()),
        PosterKind::Shelf => {
            let shelves = ui.get_home_shelves();
            shelves
                .iter()
                .enumerate()
                .flat_map(|(shelf, section)| model_rows(shelf, &section.items))
                .collect()
        }
        PosterKind::HomeExpanded => model_rows(0, &ui.global::<HomeExpanded>().get_items()),
    }
}

fn model_rows(shelf: usize, model: &ModelRc<PosterItem>) -> Vec<(usize, usize, PosterItem)> {
    model
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.poster.is_empty())
        .map(|(index, item)| (shelf, index, item))
        .collect()
}

fn model_of(ui: &CloudStream, slot: &PosterSlot) -> Option<ModelRc<PosterItem>> {
    match slot.kind {
        PosterKind::Continue => Some(ui.get_continue_items()),
        PosterKind::Search => Some(ui.get_search_items()),
        PosterKind::Browse => Some(ui.get_browse_items()),
        PosterKind::Library => Some(ui.get_library_items()),
        PosterKind::Shelf => ui
            .get_home_shelves()
            .row_data(slot.shelf)
            .map(|section| section.items),
        PosterKind::HomeExpanded => Some(ui.global::<HomeExpanded>().get_items()),
    }
}

/// Shows the artwork, unless the row now holds another title (the model was replaced meanwhile).
fn apply_image(ui: &CloudStream, slot: &PosterSlot, buffer: SharedPixelBuffer<Rgba8Pixel>) {
    let Some(model) = model_of(ui, slot) else {
        return;
    };
    let Some(mut item) = model.row_data(slot.index) else {
        return;
    };
    if !is_same_poster(item.poster.as_str(), &slot.url) {
        return;
    }
    item.image = slint::Image::from_rgba8(buffer);
    model.set_row_data(slot.index, item);
}

/// True when the row still shows the poster the artwork was loaded for.
fn is_same_poster(row_poster: &str, loaded_for: &str) -> bool {
    row_poster == loaded_for
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poster_request_headers_provider_referer_wins_over_site() {
        let own = poster_request_headers("Referer: https://cdn.test/", Some("https://site.test/"));
        assert_eq!(
            own,
            vec![("Referer".to_owned(), "https://cdn.test/".to_owned())]
        );
        let site = poster_request_headers("", Some("https://site.test/"));
        assert_eq!(
            site,
            vec![("Referer".to_owned(), "https://site.test/".to_owned())]
        );
        assert!(poster_request_headers("", None).is_empty());
    }

    #[test]
    fn is_same_poster_rejects_artwork_for_a_replaced_row() {
        assert!(is_same_poster("https://a/1.jpg", "https://a/1.jpg"));
        assert!(!is_same_poster("https://a/2.jpg", "https://a/1.jpg"));
    }
}
