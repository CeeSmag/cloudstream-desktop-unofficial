//! Ephemeral artwork hydration (poster, backdrop, visible episode stills, cast portraits) for Details.

use slint::Model;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::poster_hydrate::{provider_referer, request_ephemeral_with_referer};

/// Drops every ephemeral Details image from in-memory artwork cache when leaving or switching titles.
pub(crate) fn forget_details_artwork(ctx: &BridgeContext) {
    let urls = std::mem::take(&mut ctx.session.borrow_mut().details.ephemeral_urls);
    for url in urls {
        ctx.services.artwork.forget_memory(&url);
    }
}

/// Hydrates the poster and wide backdrop of the currently open title page.
pub(crate) fn show_details_artwork(ctx: &BridgeContext, ui: &CloudStream) {
    ui.set_details_poster(slint::Image::default());
    ui.set_details_backdrop(slint::Image::default());
    let (provider, poster_url, background_url) = {
        let session = ctx.session.borrow();
        (
            session.details.provider.clone(),
            session.details.poster_url.clone(),
            session.details.background_url.clone(),
        )
    };
    let referer = provider_referer(ctx, &provider);
    let title = ui.get_details_title();
    if let Some(url) = poster_url.as_deref().filter(|u| !u.is_empty()) {
        remember_ephemeral(ctx, url);
        let expected = title.clone();
        request_ephemeral_with_referer(ctx, url, referer.as_deref(), move |ui, image| {
            if ui.get_details_title() == expected {
                ui.set_details_poster(image);
            }
        });
    }
    if let Some(url) = background_url
        .as_deref()
        .filter(|u| !u.is_empty() && Some(*u) != poster_url.as_deref())
    {
        remember_ephemeral(ctx, url);
        request_ephemeral_with_referer(ctx, url, referer.as_deref(), move |ui, image| {
            if ui.get_details_title() == title {
                ui.set_details_backdrop(image);
            }
        });
    }
}

/// Hydrates episode thumbnails for the currently visible episode chunk (at most 20 cards).
pub(crate) fn hydrate_episode_thumbnails(ctx: &BridgeContext, ui: &CloudStream) {
    let provider = ctx.session.borrow().details.provider.clone();
    let referer = provider_referer(ctx, &provider);
    let model = ui.get_episodes();
    for (index, item) in model.iter().enumerate() {
        let url = item.poster.to_string();
        if url.is_empty() {
            continue;
        }
        remember_ephemeral(ctx, &url);
        let expected = url.clone();
        request_ephemeral_with_referer(ctx, &url, referer.as_deref(), move |ui, image| {
            let episodes = ui.get_episodes();
            if let Some(mut row) = episodes.row_data(index) {
                if row.poster.as_str() == expected {
                    row.image = image;
                    episodes.set_row_data(index, row);
                }
            }
        });
    }
}

/// Hydrates circular portraits for the cast strip (capped when populated).
pub(crate) fn hydrate_cast_portraits(ctx: &BridgeContext, ui: &CloudStream) {
    let provider = ctx.session.borrow().details.provider.clone();
    let referer = provider_referer(ctx, &provider);
    let model = ui.get_details_cast_items();
    for (index, item) in model.iter().enumerate() {
        let url = item.photo_url.to_string();
        if url.is_empty() {
            continue;
        }
        remember_ephemeral(ctx, &url);
        let expected = url.clone();
        request_ephemeral_with_referer(ctx, &url, referer.as_deref(), move |ui, image| {
            let cast = ui.get_details_cast_items();
            if let Some(mut row) = cast.row_data(index) {
                if row.photo_url.as_str() == expected {
                    row.image = image;
                    cast.set_row_data(index, row);
                }
            }
        });
    }
}

fn remember_ephemeral(ctx: &BridgeContext, url: &str) {
    let mut session = ctx.session.borrow_mut();
    if !session.details.ephemeral_urls.iter().any(|u| u == url) {
        session.details.ephemeral_urls.push(url.to_owned());
    }
}
