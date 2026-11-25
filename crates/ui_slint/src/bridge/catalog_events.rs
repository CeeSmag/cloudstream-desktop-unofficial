//! Shows start-up progress, Home, search, Browse and title pages.

use std::time::Duration;

use crate::episode_groups::{chunk_for_key, initial_season, season_episodes, season_labels};
use crate::focus_grid::FocusPoint;
use crate::home_scroll::reset_home_scroll;
use crate::pagination::pagination_model;
use crate::posters::{
    posters_from_bookmarks, posters_from_browse, posters_from_continue, posters_from_search,
    shelves_from_sections, string_model,
};
use crate::resume_hero::{pick_resume_hero, resume_facts};
use crate::startup_progress::{home_loading_view, plugin_loading_view, StartupView};
use crate::ui_events::{DetailsPage, PosterKind};
use crate::{BrowseFilter, CloudStream};
use app_services::BrowsePage;
use slint::ComponentHandle;

use super::bridge_context::BridgeContext;
use super::details_artwork::{
    forget_details_artwork, hydrate_cast_portraits, show_details_artwork,
};
use super::details_page::{cast_items_model, format_details_meta};
use super::details_view::show_episode_list;
use super::extension_catalog::refresh_plugins;
use super::focus_screens::FocusScreen;
use super::home_expanded::close_expanded_shelf;
use super::home_loading::{load_home, HomeShelf};
use super::poster_hydrate::{hydrate_poster_images, request_artwork};
use super::repository_discovery::watch_discovered_repositories;
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
            let cleanup = ctx.extras.borrow().cleanup.clone();
            slint::Timer::single_shot(STARTUP_HOME_WAIT, move || {
                if let Some(ui) = weak.upgrade() {
                    ui.set_starting(false);
                }
                if let Some(cleanup) = cleanup {
                    cleanup.notify_home_ready();
                }
            });
        }
        None => mark_home_arrived(ctx, ui),
    }
    if let Some(error) = error {
        ui.set_notice(error.into());
    }
    refresh_plugins(ctx, ui);
    show_library(ctx, ui);
    show_continue_watching(ctx, ui);
    load_home(ctx);
    watch_discovered_repositories(ctx);
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

/// Shows the continue-watching row and the banner for the selected provider's newest
/// unfinished title (none when that provider has none).
pub(crate) fn show_continue_watching(ctx: &BridgeContext, ui: &CloudStream) {
    let list = match ctx.services.library.continue_watching() {
        Ok(list) => list,
        Err(error) => {
            tracing::warn!(%error, "could not read the watch history");
            return;
        }
    };
    ui.set_continue_items(posters_from_continue(&list));
    let hero = pick_resume_hero(&list, &ui.get_selected_provider());
    ui.set_hero_index(hero.and_then(|at| i32::try_from(at).ok()).unwrap_or(-1));
    let Some(item) = hero.and_then(|at| list.get(at)) else {
        ui.set_hero_title("".into());
        hydrate_poster_images(ctx, PosterKind::Continue);
        return;
    };
    ui.set_hero_title(item.title.clone().into());
    ui.set_hero_subtitle(resume_facts(item).into());
    ui.set_hero_image(slint::Image::default());
    let title = item.title.clone();
    request_artwork(
        ctx,
        item.poster_url.as_deref().unwrap_or_default(),
        move |ui, image| {
            if ui.get_hero_title() == title.as_str() {
                ui.set_hero_image(image);
            }
        },
    );
    hydrate_poster_images(ctx, PosterKind::Continue);
}

pub(crate) fn show_home(ctx: &BridgeContext, ui: &CloudStream, sections: &[HomeShelf]) {
    close_expanded_shelf(ctx);
    // New shelves: an old position would point somewhere else in them.
    reset_home_scroll(ui);
    ctx.session.borrow_mut().home_shelves = sections.to_vec();
    ui.set_home_loading(false);
    mark_home_arrived(ctx, ui);
    ui.set_home_shelves(shelves_from_sections(sections));
    hydrate_poster_images(ctx, PosterKind::Shelf);
}

fn mark_home_arrived(ctx: &BridgeContext, ui: &CloudStream) {
    ui.set_starting(false);
    if let Some(cleanup) = ctx.extras.borrow().cleanup.as_ref() {
        cleanup.notify_home_ready();
    }
    super::webview_dialog::on_home_arrived(ctx, ui);
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
    result: Result<BrowsePage, String>,
) {
    ui.set_browse_loading(false);
    let page = match result {
        Ok(page) => page,
        Err(error) => {
            ui.set_browse_message(format!("Browse could not load: {error}").into());
            return;
        }
    };
    let items = {
        let mut session = ctx.session.borrow_mut();
        session.browse_items = page.items;
        session.browse_page = (page.page, page.total_pages);
        session
            .focus
            .entry(FocusScreen::Browse)
            .or_default()
            .set(FocusPoint { row: 0, col: 0 });
        session.browse_items.clone()
    };
    if ui.get_tab() == 1 && ui.get_details_title().is_empty() {
        ui.set_focus_row(0);
        ui.set_focus_col(0);
    }
    let message = if items.is_empty() {
        "Nothing matches these filters."
    } else {
        ""
    };
    ui.set_browse_message(message.into());
    ui.set_browse_items(posters_from_browse(&items));
    let filter = ui.global::<BrowseFilter>();
    filter.set_current_page(i32::try_from(page.page).unwrap_or(1));
    filter.set_total_pages(i32::try_from(page.total_pages).unwrap_or(1));
    filter.set_pages(pagination_model(page.page, page.total_pages));
    filter.set_scroll_reset(filter.get_scroll_reset().wrapping_add(1));
    hydrate_poster_images(ctx, PosterKind::Browse);
}

/// Tags shown on a title page (providers can send dozens).
const MAX_TAGS: usize = 8;

pub(crate) fn show_details(ctx: &BridgeContext, ui: &CloudStream, page: DetailsPage) {
    forget_details_artwork(ctx);
    let bookmarked = ctx
        .services
        .library
        .is_bookmarked(&page.provider, &page.url);
    let season = initial_season(&page.episodes, page.resume_media_key.as_deref());
    let resume_chunk = page.resume_media_key.as_deref().and_then(|key| {
        let sorted = season_episodes(&page.episodes, season, true);
        chunk_for_key(&sorted, key)
    });
    let meta_info = format_details_meta(&page);
    let cast_model = cast_items_model(&page.cast_members);
    store_details_session(ctx, &page, resume_chunk.unwrap_or(0));
    ui.set_details_loading(false);
    ui.set_details_bookmarked(bookmarked.unwrap_or(false));
    ui.set_details_title(page.title.into());
    ui.set_details_plot(page.plot.into());
    ui.set_details_meta_info(meta_info);
    ui.set_details_tags(string_model(page.tags.into_iter().take(MAX_TAGS)));
    ui.set_details_cast_items(cast_model);
    ui.set_resume_label(page.resume.into());
    ui.set_season_filter(season);
    ui.set_season_labels(string_model(season_labels(&page.episodes)));
    ui.set_details_resume_key(page.resume_media_key.unwrap_or_default().into());
    show_episode_list(ctx, ui);
    show_details_artwork(ctx, ui);
    hydrate_cast_portraits(ctx, ui);
}

fn store_details_session(ctx: &BridgeContext, page: &DetailsPage, episode_chunk: usize) {
    let mut session = ctx.session.borrow_mut();
    let poster_url = page
        .poster_url
        .clone()
        .or_else(|| session.details.poster_url.clone());
    session.details = DetailsTarget {
        provider: page.provider.clone(),
        url: page.url.clone(),
        data: page.data.clone(),
        media_key: page.media_key.clone(),
        poster_url,
        background_url: page.background_url.clone(),
        resume_media_key: page.resume_media_key.clone(),
        skip_ids: page.skip_ids.clone(),
        media_type: page.media_type.clone(),
        episodes_descending: false,
        episode_chunk,
        ephemeral_urls: Vec::new(),
    };
    session.episodes.clone_from(&page.episodes);
}
