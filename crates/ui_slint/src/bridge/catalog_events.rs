//! Shows start-up progress, Home, search, Browse and title pages.

use std::time::Duration;

use slint::{ModelRc, VecModel};

use crate::posters::{
    posters_from_bookmarks, posters_from_browse, posters_from_continue, posters_from_search,
    shelves_from_sections, string_model,
};
use crate::startup_progress::{home_loading_view, plugin_loading_view, StartupView};
use crate::ui_events::{DetailsPage, PosterKind};
use crate::{CloudStream, EpisodeItem};
use app_services::BrowseItem;

use super::bridge_context::BridgeContext;
use super::details_loading::season_labels;
use super::extension_settings_callbacks::refresh_plugins;
use super::home_loading::load_home;
use super::poster_hydrate::hydrate_poster_images;
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
    match providers.first() {
        Some(first) => {
            ui.set_selected_provider(first.into());
            show_startup_view(ui, &home_loading_view(first));
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

fn show_library(ctx: &BridgeContext, ui: &CloudStream) {
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
        ui.set_hero_subtitle(subtitle.into());
    }
    hydrate_poster_images(ctx, PosterKind::Continue);
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

pub(crate) fn show_details(ctx: &BridgeContext, ui: &CloudStream, page: DetailsPage) {
    ctx.session.borrow_mut().details = DetailsTarget {
        provider: page.provider,
        url: page.url,
        data: page.data,
        media_key: page.media_key,
    };
    ui.set_details_loading(false);
    ui.set_details_title(page.title.into());
    ui.set_details_plot(page.plot.into());
    ui.set_details_meta(page.meta.into());
    ui.set_resume_label(page.resume.into());
    ui.set_season_filter(-1);
    ui.set_season_labels(string_model(season_labels(&page.episodes)));
    let rows: Vec<EpisodeItem> = page
        .episodes
        .into_iter()
        .map(|row| EpisodeItem {
            name: row.name.into(),
            code: row.code.into(),
            data: row.data.into(),
            watched: row.watched,
            season: row.season,
        })
        .collect();
    ui.set_episodes(ModelRc::new(VecModel::from(rows)));
}
