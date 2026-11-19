//! Fills poster models with their artwork, loaded by the artwork service off the UI thread.

use std::collections::HashMap;

use poster_artwork::DecodedArtwork;
use slint::{Model, ModelRc, Rgba8Pixel, SharedPixelBuffer};

use crate::posters::decode_poster_headers;
use crate::ui_events::PosterKind;
use crate::{CloudStream, PosterItem};

use super::bridge_context::BridgeContext;

/// Where one poster sits: the model, the shelf (0 outside Home), the row and its address.
#[derive(Debug, Clone)]
struct PosterSlot {
    kind: PosterKind,
    shelf: usize,
    index: usize,
    url: String,
}

/// Starts loading the artwork of every poster in the `kind` model.
pub(crate) fn hydrate_poster_images(ctx: &BridgeContext, kind: PosterKind) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let referers = provider_referers(ctx);
    for (shelf, index, item) in poster_rows(&ui, kind) {
        let url = item.poster.to_string();
        let slot = PosterSlot {
            kind,
            shelf,
            index,
            url: url.clone(),
        };
        if let Some(artwork) = ctx.services.artwork.cached(&url) {
            apply_image(&ui, &slot, pixel_buffer(&artwork));
            continue;
        }
        let referer = referers.get(item.provider.as_str()).map(String::as_str);
        let headers = poster_request_headers(&item.poster_headers, referer);
        let weak = ctx.ui.clone();
        ctx.services.artwork.request(
            &url,
            headers,
            Box::new(move |result| {
                let Ok(artwork) = result else {
                    return; // the loader logged why; the tile keeps its placeholder
                };
                let buffer = pixel_buffer(&artwork);
                let shown = weak.upgrade_in_event_loop(move |ui| apply_image(&ui, &slot, buffer));
                if let Err(error) = shown {
                    tracing::debug!(%error, "window closed before a poster arrived");
                }
            }),
        );
    }
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

/// Loads one image (a hero or a title page's artwork) and hands it to `on_ready` on the UI thread.
pub(crate) fn request_artwork(
    ctx: &BridgeContext,
    url: &str,
    on_ready: impl FnOnce(&CloudStream, slint::Image) + Send + 'static,
) {
    request_artwork_mode(ctx, url, true, on_ready);
}

/// Details poster: memory only, never written to `artwork/` on disk.
pub(crate) fn request_ephemeral_artwork(
    ctx: &BridgeContext,
    url: &str,
    on_ready: impl FnOnce(&CloudStream, slint::Image) + Send + 'static,
) {
    request_artwork_mode(ctx, url, false, on_ready);
}

fn request_artwork_mode(
    ctx: &BridgeContext,
    url: &str,
    persist: bool,
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
    if persist {
        ctx.services.artwork.request(url, Vec::new(), callback);
    } else {
        ctx.services
            .artwork
            .request_ephemeral(url, Vec::new(), callback);
    }
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
