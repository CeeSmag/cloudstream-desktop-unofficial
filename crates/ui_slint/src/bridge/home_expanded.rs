//! Home's expanded shelf view: vertical grid, page cache, and provider pagination.

use std::collections::HashMap;

use core_model::{HomePage, SearchResult};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::background_work::spawn_work;
use crate::focus_grid::{FocusGrid, FocusPoint};
use crate::posters::posters_from_search;
use crate::request_tickets::RequestKind;
use crate::ui_events::{HomeSectionPage, PosterKind, UiEvent};
use crate::{CloudStream, HomeExpanded, PosterItem};

use super::bridge_context::BridgeContext;
use super::details_loading::load_details;
use super::focus_screens::FocusScreen;
use super::home_callbacks::row;
use super::home_loading::HomeShelf;
use super::poster_hydrate::hydrate_poster_images;

/// State kept while a Home shelf is expanded into its vertical grid view.
#[derive(Debug, Clone)]
pub(crate) struct ExpandedShelfState {
    pub(crate) shelf_index: usize,
    pub(crate) page: u32,
    pub(crate) pages: HashMap<u32, (Vec<SearchResult>, bool)>,
}

/// Page metadata shown on the expanded shelf header and pager.
#[derive(Debug, Clone, Copy)]
struct PageMeta {
    page: u32,
    has_next: bool,
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

/// Opens shelf `shelf_index` in the full-page vertical grid, reusing already-hydrated posters.
pub(crate) fn open_expanded_shelf(ctx: &BridgeContext, shelf_index: usize) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let shelf = {
        let mut session = ctx.session.borrow_mut();
        let Some(shelf) = session.home_shelves.get(shelf_index).cloned() else {
            return;
        };
        session.tickets.cancel(RequestKind::HomeSection);
        let mut pages = HashMap::new();
        pages.insert(1, (shelf.items.clone(), shelf.has_next));
        session.home_expanded = Some(ExpandedShelfState {
            shelf_index,
            page: 1,
            pages,
        });
        reset_home_focus(&mut session.focus, &ui);
        shelf
    };
    let items = existing_shelf_posters(&ui, shelf_index)
        .unwrap_or_else(|| posters_from_search(&shelf.items));
    let expanded = ui.global::<HomeExpanded>();
    expanded.set_title(shelf.title.into());
    expanded.set_open(true);
    apply_page_view(
        &ui,
        items,
        PageMeta {
            page: 1,
            has_next: shelf.has_next,
        },
    );
    hydrate_poster_images(ctx, PosterKind::HomeExpanded);
}

/// Closes the expanded Home view and returns to the horizontal shelves.
pub(crate) fn close_expanded_shelf(ctx: &BridgeContext) {
    {
        let mut session = ctx.session.borrow_mut();
        session.tickets.cancel(RequestKind::HomeSection);
        session.home_expanded = None;
    }
    if let Some(ui) = ctx.window() {
        let expanded = ui.global::<HomeExpanded>();
        expanded.set_open(false);
        expanded.set_loading(false);
        expanded.set_message("".into());
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
    Cached(Vec<SearchResult>, PageMeta),
    Fetch(HomeShelf, usize, u32),
}

fn step_expanded_page(ctx: &BridgeContext, delta: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    if ui.global::<HomeExpanded>().get_loading() {
        return;
    }
    let Some(step) = resolve_page_step(ctx, &ui, delta) else {
        return;
    };
    match step {
        PageStep::Cached(items, meta) => {
            apply_page_view(&ui, posters_from_search(&items), meta);
            hydrate_poster_images(ctx, PosterKind::HomeExpanded);
        }
        PageStep::Fetch(shelf, shelf_index, target) => {
            fetch_section_page(ctx, shelf, shelf_index, target);
        }
    }
}

fn resolve_page_step(ctx: &BridgeContext, ui: &CloudStream, delta: i32) -> Option<PageStep> {
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
    if let Some((items, has_next)) = state.pages.get(&target).cloned() {
        state.page = target;
        reset_home_focus(&mut session.focus, ui);
        let meta = PageMeta {
            page: target,
            has_next,
        };
        return Some(PageStep::Cached(items, meta));
    }
    let shelf_index = state.shelf_index;
    let shelf = session.home_shelves.get(shelf_index).cloned()?;
    Some(PageStep::Fetch(shelf, shelf_index, target))
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

/// Applies a loaded page of the currently expanded Home shelf.
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
            expanded.set_message(format!("Could not load page {}: {error}", loaded.page).into());
            return;
        }
    };
    let applied = {
        let mut session = ctx.session.borrow_mut();
        let Some(state) = session.home_expanded.as_mut() else {
            return;
        };
        if state.shelf_index != loaded.shelf_index {
            return;
        }
        if items.is_empty() && loaded.page > 1 {
            if let Some(current) = state.pages.get_mut(&state.page) {
                current.1 = false;
            }
            return show_end_of_section(ui);
        }
        state.pages.insert(loaded.page, (items.clone(), has_next));
        state.page = loaded.page;
        reset_home_focus(&mut session.focus, ui);
        true
    };
    if applied {
        apply_page_view(
            ui,
            posters_from_search(&items),
            PageMeta {
                page: loaded.page,
                has_next,
            },
        );
        hydrate_poster_images(ctx, PosterKind::HomeExpanded);
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
    expanded.set_scroll_reset(expanded.get_scroll_reset().wrapping_add(1));
}

fn existing_shelf_posters(ui: &CloudStream, shelf_index: usize) -> Option<ModelRc<PosterItem>> {
    let section = ui.get_home_shelves().row_data(shelf_index)?;
    let cloned: Vec<PosterItem> = section.items.iter().collect();
    Some(ModelRc::new(VecModel::from(cloned)))
}

fn reset_home_focus(focus: &mut HashMap<FocusScreen, FocusGrid>, ui: &CloudStream) {
    let start = FocusPoint { row: 0, col: 0 };
    focus.entry(FocusScreen::Home).or_default().set(start);
    if ui.get_tab() == 0 && ui.get_details_title().is_empty() {
        ui.set_focus_row(0);
        ui.set_focus_col(0);
    }
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
mod tests {
    use core_model::HomePageSection;

    use super::*;

    fn section(name: &str, titles: &[&str]) -> HomePageSection {
        HomePageSection {
            name: name.to_owned(),
            list: titles
                .iter()
                .map(|title| SearchResult {
                    name: (*title).to_owned(),
                    ..SearchResult::default()
                })
                .collect(),
            is_horizontal_images: false,
        }
    }

    #[test]
    fn pick_section_items_matches_by_title_ignoring_case() {
        let page = HomePage {
            items: vec![
                section("Trending", &["A"]),
                section("Popular Movies", &["Dune", "Heat"]),
            ],
            has_next: true,
        };
        let (list, has_next) = pick_section_items(page, "popular movies", 0);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Dune");
        assert!(has_next);
    }

    #[test]
    fn pick_section_items_falls_back_to_single_section_or_index() {
        let single = HomePage {
            items: vec![section("", &["Only"])],
            has_next: true,
        };
        let (list, has_next) = pick_section_items(single, "Popular Movies", 2);
        assert_eq!(list.len(), 1);
        assert!(has_next);

        let empty = HomePage {
            items: vec![section("Popular Movies", &[])],
            has_next: true,
        };
        let (list, has_next) = pick_section_items(empty, "Popular Movies", 0);
        assert!(list.is_empty());
        assert!(!has_next, "empty trailing page clears has_next");
    }

    #[test]
    fn expanded_count_text_formats_paged_and_unpaged_counts() {
        assert_eq!(expanded_count_text(1, 1, false), "1 title");
        assert_eq!(expanded_count_text(20, 1, false), "20 titles");
        assert_eq!(expanded_count_text(20, 1, true), "Page 1 · 20 titles");
        assert_eq!(expanded_count_text(12, 3, false), "Page 3 · 12 titles");
    }
}
