//! Home's expanded shelf view: vertical grid, page cache, and provider pagination.

use core_model::{HomePage, SearchResult};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::background_work::spawn_work;
use crate::poster_limits::EXPANDED_PAGE_SIZE;
use crate::posters::{poster_rows_from_search, posters_from_search};
use crate::request_tickets::RequestKind;
use crate::shelf_pager::ShelfPager;
use crate::ui_events::{HomeSectionPage, PosterKind, UiEvent};
use crate::{CloudStream, HomeExpanded, PosterItem};

use super::bridge_context::BridgeContext;
use super::details_loading::load_details;
use super::home_callbacks::row;
use super::home_loading::HomeShelf;
use super::poster_hydrate::{hydrate_poster_images, release_poster_images};

/// State kept while a Home shelf is expanded into its vertical grid view.
#[derive(Debug, Clone)]
pub(crate) struct ExpandedShelfState {
    pub(crate) shelf_index: usize,
    /// The page on screen (1-based, pages of `EXPANDED_PAGE_SIZE`).
    pub(crate) page: u32,
    /// The page the viewer asked for while the provider's next page is being fetched.
    pub(crate) awaiting: Option<u32>,
    pub(crate) pager: ShelfPager,
}

/// Page metadata shown on the expanded shelf header and pager.
#[derive(Debug, Clone, Copy)]
struct PageMeta {
    page: u32,
    has_next: bool,
}

/// What a provider page that arrived means for the expanded view.
enum LoadedPage {
    /// Show these titles as the new page.
    Show(Vec<SearchResult>, PageMeta),
    /// The provider had nothing more: stay on the last page and say so.
    End,
    /// Nobody waits for it (collapsed, replaced, repeated): drop it silently.
    Ignored,
}

/// Connects the `HomeExpanded` global's callbacks to the bridge.
pub(crate) fn wire_home_expanded(ctx: &BridgeContext, ui: &CloudStream) {
    let expanded = ui.global::<HomeExpanded>();
    let c = ctx.clone();
    expanded.on_open_shelf(move |index| open_expanded_shelf(&c, row(index)));
    let c = ctx.clone();
    expanded.on_close(move || close_expanded_shelf(&c));
    let c = ctx.clone();
    expanded.on_open_item(move |index| open_expanded_item(&c, row(index)));
    let c = ctx.clone();
    expanded.on_prev_page(move || step_expanded_page(&c, -1));
    let c = ctx.clone();
    expanded.on_next_page(move || step_expanded_page(&c, 1));
}

/// Expands shelf `shelf_index` inline on Home: page 1 of its titles, reusing the pictures the
/// collapsed row already loaded.
pub(crate) fn open_expanded_shelf(ctx: &BridgeContext, shelf_index: usize) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let Some((titles, meta)) = begin_expanded(ctx, shelf_index) else {
        return;
    };
    let mut rows = poster_rows_from_search(&titles);
    if let Some(section) = ui.get_home_shelves().row_data(shelf_index) {
        reuse_loaded_pictures(&mut rows, &section.items);
    }
    tracing::debug!(shelf_index, posters = rows.len(), "expanded a Home shelf");
    ui.global::<HomeExpanded>()
        .set_shelf_index(i32::try_from(shelf_index).unwrap_or(-1));
    apply_page_view(&ui, ModelRc::new(VecModel::from(rows)), meta);
    hydrate_poster_images(ctx, PosterKind::HomeExpanded);
}

/// Starts the expanded state for a shelf and returns page 1's titles.
fn begin_expanded(
    ctx: &BridgeContext,
    shelf_index: usize,
) -> Option<(Vec<SearchResult>, PageMeta)> {
    let mut session = ctx.session.borrow_mut();
    let pager = {
        let shelf = session.home_shelves.get(shelf_index)?;
        ShelfPager::new(shelf.items.clone(), shelf.has_next, EXPANDED_PAGE_SIZE)
    };
    let titles = pager.page(1).map(<[SearchResult]>::to_vec)?;
    let meta = PageMeta {
        page: 1,
        has_next: pager.has_page_after(1),
    };
    session.tickets.cancel(RequestKind::HomeSection);
    session.home_expanded = Some(ExpandedShelfState {
        shelf_index,
        page: 1,
        awaiting: None,
        pager,
    });
    Some((titles, meta))
}

/// Copies the pictures of the collapsed row onto the same posters of the expanded page.
fn reuse_loaded_pictures(rows: &mut [PosterItem], collapsed: &ModelRc<PosterItem>) {
    for (row, loaded) in rows.iter_mut().zip(collapsed.iter()) {
        if row.poster == loaded.poster {
            row.image = loaded.image.clone();
        }
    }
}

/// Collapses the inline expanded shelf so lower shelves reappear on Home. The expanded page's
/// posters are dropped with its model, so their pictures stop being held by the screen.
pub(crate) fn close_expanded_shelf(ctx: &BridgeContext) {
    {
        let mut session = ctx.session.borrow_mut();
        session.tickets.cancel(RequestKind::HomeSection);
        session.home_expanded = None;
    }
    release_poster_images(ctx, PosterKind::HomeExpanded);
    if let Some(ui) = ctx.window() {
        let expanded = ui.global::<HomeExpanded>();
        expanded.set_shelf_index(-1);
        expanded.set_items(ModelRc::new(VecModel::from(Vec::<PosterItem>::new())));
        expanded.set_loading(false);
        expanded.set_message("".into());
        expanded.set_count_text("".into());
        expanded.set_page(1);
        expanded.set_has_prev(false);
        expanded.set_has_next(false);
    }
}

fn open_expanded_item(ctx: &BridgeContext, index: usize) {
    let item = ctx
        .window()
        .and_then(|ui| ui.global::<HomeExpanded>().get_items().row_data(index));
    if let Some(item) = item {
        load_details(ctx, &item);
    }
}

enum PageStep {
    Show(Vec<SearchResult>, PageMeta),
    Fetch(HomeShelf, usize, u32),
}

fn step_expanded_page(ctx: &BridgeContext, delta: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    if ui.global::<HomeExpanded>().get_loading() {
        return;
    }
    let Some(step) = resolve_page_step(ctx, delta) else {
        return;
    };
    match step {
        PageStep::Show(titles, meta) => {
            tracing::debug!(
                page = meta.page,
                posters = titles.len(),
                "Home shelf page shown"
            );
            apply_page_view(&ui, posters_from_search(&titles), meta);
            hydrate_poster_images(ctx, PosterKind::HomeExpanded);
        }
        PageStep::Fetch(shelf, shelf_index, provider_page) => {
            fetch_section_page(ctx, shelf, shelf_index, provider_page);
        }
    }
}

fn resolve_page_step(ctx: &BridgeContext, delta: i32) -> Option<PageStep> {
    let mut session = ctx.session.borrow_mut();
    let state = session.home_expanded.as_mut()?;
    let target = if delta < 0 {
        state.page.saturating_sub(1)
    } else {
        state.page.saturating_add(1)
    };
    if target < 1 || target == state.page {
        return None;
    }
    if let Some(titles) = state.pager.page(target).map(<[SearchResult]>::to_vec) {
        state.page = target;
        let has_next = state.pager.has_page_after(target);
        return Some(PageStep::Show(
            titles,
            PageMeta {
                page: target,
                has_next,
            },
        ));
    }
    if !state.pager.needs_fetch_for(target) {
        return None;
    }
    state.awaiting = Some(target);
    let (shelf_index, provider_page) = (state.shelf_index, state.pager.next_provider_page());
    let shelf = session.home_shelves.get(shelf_index).cloned()?;
    Some(PageStep::Fetch(shelf, shelf_index, provider_page))
}

fn fetch_section_page(ctx: &BridgeContext, shelf: HomeShelf, shelf_index: usize, page: u32) {
    if let Some(ui) = ctx.window() {
        let expanded = ui.global::<HomeExpanded>();
        expanded.set_loading(true);
        expanded.set_message("".into());
    }
    let ticket = ctx.issue_ticket(RequestKind::HomeSection);
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("home-section", move || {
        let result = services
            .catalog
            .home_page(&shelf.provider, page, &shelf.entry)
            .map_err(|error| error.to_string())
            .map(|reply| match reply {
                Some(home_page) => pick_section_items(home_page, &shelf.title, shelf.section_index),
                None => (Vec::new(), false),
            });
        sender.send(UiEvent::HomeSectionPageLoaded(
            ticket,
            HomeSectionPage {
                shelf_index,
                page,
                result,
            },
        ));
    });
}

/// Applies a provider page that arrived for the currently expanded Home shelf.
pub(crate) fn show_home_section_page(
    ctx: &BridgeContext,
    ui: &CloudStream,
    loaded: HomeSectionPage,
) {
    let expanded = ui.global::<HomeExpanded>();
    expanded.set_loading(false);
    let (items, has_next) = match loaded.result {
        Ok(data) => data,
        Err(error) => {
            clear_awaiting(ctx);
            expanded.set_message(format!("Could not load more titles: {error}").into());
            return;
        }
    };
    let outcome = {
        let mut session = ctx.session.borrow_mut();
        let Some(state) = session.home_expanded.as_mut() else {
            return;
        };
        if state.shelf_index != loaded.shelf_index {
            return;
        }
        accept_loaded_page(state, loaded.page, items, has_next)
    };
    match outcome {
        LoadedPage::Show(titles, meta) => {
            apply_page_view(ui, posters_from_search(&titles), meta);
            hydrate_poster_images(ctx, PosterKind::HomeExpanded);
        }
        LoadedPage::End => show_end_of_section(ui),
        LoadedPage::Ignored => {}
    }
}

/// Adds a delivered provider page to the shelf's pager and decides what the viewer sees.
fn accept_loaded_page(
    state: &mut ExpandedShelfState,
    provider_page: u32,
    items: Vec<SearchResult>,
    has_next: bool,
) -> LoadedPage {
    let Some(target) = state.awaiting else {
        return LoadedPage::Ignored;
    };
    if !state.pager.append(provider_page, items, has_next) {
        return LoadedPage::Ignored;
    }
    state.awaiting = None;
    match state.pager.page(target).map(<[SearchResult]>::to_vec) {
        Some(titles) => {
            state.page = target;
            let has_next = state.pager.has_page_after(target);
            LoadedPage::Show(
                titles,
                PageMeta {
                    page: target,
                    has_next,
                },
            )
        }
        None => LoadedPage::End,
    }
}

/// Forgets the page being waited for (the fetch failed), so Next can try again.
fn clear_awaiting(ctx: &BridgeContext) {
    if let Some(state) = ctx.session.borrow_mut().home_expanded.as_mut() {
        state.awaiting = None;
    }
}

fn show_end_of_section(ui: &CloudStream) {
    let expanded = ui.global::<HomeExpanded>();
    expanded.set_has_next(false);
    expanded.set_message("No more titles in this section.".into());
}

fn apply_page_view(ui: &CloudStream, items: ModelRc<PosterItem>, meta: PageMeta) {
    let count = items.row_count();
    let expanded = ui.global::<HomeExpanded>();
    expanded.set_loading(false);
    expanded.set_message("".into());
    expanded.set_items(items);
    expanded.set_page(i32::try_from(meta.page).unwrap_or(1));
    expanded.set_has_prev(meta.page > 1);
    expanded.set_has_next(meta.has_next);
    expanded.set_count_text(expanded_count_text(count, meta.page, meta.has_next).into());
}

/// Extracts the section matching `title` (or fallback index) from a provider's paged `HomePage`.
pub(crate) fn pick_section_items(
    page: HomePage,
    title: &str,
    section_index: usize,
) -> (Vec<SearchResult>, bool) {
    let has_next = page.has_next;
    let items = page.items;
    let chosen = items
        .iter()
        .position(|section| section.name.eq_ignore_ascii_case(title))
        .or_else(|| (items.len() == 1).then_some(0))
        .or_else(|| (section_index < items.len()).then_some(section_index));
    let list = chosen
        .and_then(|at| items.into_iter().nth(at))
        .map(|section| section.list)
        .unwrap_or_default();
    let more = has_next && !list.is_empty();
    (list, more)
}

/// Compact badge label for the expanded section header.
pub(crate) fn expanded_count_text(count: usize, page: u32, has_next: bool) -> String {
    let noun = if count == 1 { "title" } else { "titles" };
    if page > 1 || has_next {
        format!("Page {page} · {count} {noun}")
    } else {
        format!("{count} {noun}")
    }
}

#[cfg(test)]
#[path = "home_expanded_tests.rs"]
mod tests;
