//! Shows start-up progress, Home, search, Browse and title pages.

use std::time::Duration;

use crate::posters::{
    posters_from_bookmarks, posters_from_browse, posters_from_continue, posters_from_search,
    shelves_from_sections, string_model,
};
use crate::startup_progress::{home_loading_view, plugin_loading_view, StartupView};
use crate::ui_events::{DetailsPage, PosterKind};
use crate::CloudStream;
use app_services::BrowseItem;

use super::bridge_context::BridgeContext;
use super::details_loading::season_labels;
use super::details_view::{show_details_artwork, show_episode_list};
use super::extension_catalog::refresh_plugins;
use super::home_loading::load_home;
use super::poster_hydrate::{hydrate_poster_images, request_artwork};
use super::session::DetailsTarget;
use super::title_search::{search_message, SearchSummary};

/// Longest the start-up screen waits for Home after the plugins loaded; then Home shows its own
/// loading state.
const STARTUP_HOME_WAIT: Duration = Duration::from_secs(20);

pub(crate) fn show_startup_progress(
    ui: &CloudStream,
    finished: usize,
    total: usize,
    current: &str,
) {
    show_startup_view(ui, &plugin_loading_view(finished, total, current));
}

fn show_startup_view(ui: &CloudStream, view: &StartupView) {
    ui.set_startup_status(view.label.clone().into());
    ui.set_startup_progress(view.fraction);
    ui.set_startup_indeterminate(view.indeterminate);
}

pub(crate) fn show_startup_done(
    ctx: &BridgeContext,
    ui: &CloudStream,
    providers: &[String],
    error: Option<String>,
) {
    ui.set_providers(string_model(providers.iter().cloned()));
    let saved = ctx.services.settings.home_provider().ok().flatten();
    let chosen = saved
        .filter(|name| providers.contains(name))
        .or_else(|| providers.first().cloned());
    match chosen {
        Some(first) => {
            ui.set_selected_provider(first.clone().into());
            show_startup_view(ui, &home_loading_view(&first));
            let weak = ctx.ui.clone();
            slint::Timer::single_shot(STARTUP_HOME_WAIT, move || {
                if let Some(ui) = weak.upgrade() {
                    ui.set_starting(false);
                }
            });
        }
        None => ui.set_starting(false),
    }
    if let Some(error) = error {
        ui.set_notice(error.into());
    }
    refresh_plugins(ctx, ui);
    show_library(ctx, ui);
    show_continue_watching(ctx, ui);
    load_home(ctx);
}

/// Shows the bookmarked titles in the Library.
pub(crate) fn show_library(ctx: &BridgeContext, ui: &CloudStream) {
    match ctx.services.library.bookmarks() {
        Ok(bookmarks) => {
            ui.set_library_items(posters_from_bookmarks(&bookmarks));
            hydrate_poster_images(ctx, PosterKind::Library);
        }
        Err(error) => tracing::warn!(%error, "could not read the library"),
    }
}

/// Shows the continue-watching row and the hero for its newest title.
pub(crate) fn show_continue_watching(ctx: &BridgeContext, ui: &CloudStream) {
    let list = match ctx.services.library.continue_watching() {
        Ok(list) => list,
        Err(error) => {
            tracing::warn!(%error, "could not read the watch history");
            return;
        }
    };
    ui.set_continue_items(posters_from_continue(&list));
    if let Some(first) = list.first() {
        ui.set_hero_title(first.title.clone().into());
        let subtitle = match (first.season, first.episode) {
            (Some(season), Some(episode)) => format!("S{season} E{episode}"),
            _ => first.provider.clone(),
        };
        let left = watched_left(first.position_ms, first.duration_ms);
        ui.set_hero_subtitle(format!("{subtitle}{left}").into());
        ui.set_hero_image(slint::Image::default());
        let title = first.title.clone();
        request_artwork(
            ctx,
            first.poster_url.as_deref().unwrap_or_default(),
            move |ui, image| {
                if ui.get_hero_title() == title.as_str() {
                    ui.set_hero_image(image);
                }
            },
        );
    }
    hydrate_poster_images(ctx, PosterKind::Continue);
}

/// "  ·  23 min left" when the length is known.
fn watched_left(position_ms: i64, duration_ms: i64) -> String {
    let minutes = (duration_ms - position_ms) / 60_000;
    if duration_ms > 0 && minutes > 0 {
        format!("  ·  {minutes} min left")
    } else {
        String::new()
    }
}

pub(crate) fn show_home(
    ctx: &BridgeContext,
    ui: &CloudStream,
    sections: &[(String, Vec<core_model::SearchResult>)],
) {
    ui.set_home_loading(false);
    ui.set_starting(false);
    ui.set_home_shelves(shelves_from_sections(sections));
    hydrate_poster_images(ctx, PosterKind::Shelf);
}

pub(crate) fn show_search(ctx: &BridgeContext, ui: &CloudStream, summary: &SearchSummary) {
    ui.set_search_loading(false);
    ui.set_search_message(search_message(summary).into());
    ui.set_search_items(posters_from_search(&summary.items));
    hydrate_poster_images(ctx, PosterKind::Search);
}

pub(crate) fn show_browse(
    ctx: &BridgeContext,
    ui: &CloudStream,
    result: Result<Vec<BrowseItem>, String>,
) {
    ui.set_browse_loading(false);
    let items = match result {
        Ok(items) => items,
        Err(error) => {
            ui.set_browse_message(format!("Browse could not load: {error}").into());
            return;
        }
    };
    let message = if items.is_empty() {
        "Nothing to show."
    } else {
        ""
    };
    ui.set_browse_message(message.into());
    ui.set_browse_items(posters_from_browse(&items));
    ctx.session.borrow_mut().browse_items = items;
    hydrate_poster_images(ctx, PosterKind::Browse);
}

/// Tags shown on a title page (providers can send dozens).
const MAX_TAGS: usize = 8;

pub(crate) fn show_details(ctx: &BridgeContext, ui: &CloudStream, page: DetailsPage) {
    let bookmarked = ctx
        .services
        .library
        .is_bookmarked(&page.provider, &page.url);
    {
        let mut session = ctx.session.borrow_mut();
        let poster_url = page
            .poster_url
            .or_else(|| session.details.poster_url.clone());
        session.details = DetailsTarget {
            provider: page.provider,
            url: page.url,
            data: page.data,
            media_key: page.media_key,
            poster_url,
            resume_media_key: page.resume_media_key.clone(),
        };
        session.episodes.clone_from(&page.episodes);
    }
    ui.set_details_loading(false);
    ui.set_details_bookmarked(bookmarked.unwrap_or(false));
    ui.set_details_title(page.title.into());
    ui.set_details_plot(page.plot.into());
    ui.set_details_meta(page.meta.into());
    ui.set_details_tags(string_model(page.tags.into_iter().take(MAX_TAGS)));
    ui.set_details_cast(page.cast.join("  ·  ").into());
    ui.set_resume_label(page.resume.into());
    ui.set_season_filter(-1);
    ui.set_season_labels(string_model(season_labels(&page.episodes)));
    ui.set_details_resume_key(page.resume_media_key.clone().unwrap_or_default().into());
    show_episode_list(ctx, ui);
    show_details_artwork(ctx, ui);
}
