//! Remote / keyboard navigation: arrows move the focus, Enter opens, Back closes or goes up.

use slint::{ComponentHandle, Model};

use crate::episode_groups::MAX_SEASON_TABS;
use crate::focus_grid::{Direction, FocusGrid, FocusPoint, NAV_ROW};
use crate::{CloudStream, HomeExpanded, RepoDetail, SecurityDialog};

use super::bridge_context::BridgeContext;
use super::focus_screens::{
    current_screen, screen_rows, DetailsAction, DetailsButtons, FocusScreen,
};
use super::library_actions::{remove_bookmark, remove_continue};

/// Tabs in the top bar.
const NAV_ITEMS: usize = 5;

pub(crate) fn wire_navigation(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_nav_key(move |key| {
        if let Some(ui) = c.window() {
            handle_key(&c, &ui, key.as_str());
        }
    });
}

fn handle_key(ctx: &BridgeContext, ui: &CloudStream, key: &str) {
    if super::webview_dialog::dialog_takes_key(ctx, ui, key) {
        return;
    }
    let security_dialog = ui.global::<SecurityDialog>();
    if security_dialog.get_open() {
        if key == "back" {
            security_dialog.invoke_cancel();
        }
        return;
    }
    let Some(screen) = current_screen(ui) else {
        return;
    };
    let direction = match key {
        "up" => Direction::Up,
        "down" => Direction::Down,
        "left" => Direction::Left,
        "right" => Direction::Right,
        "enter" => return activate(ctx, ui, screen),
        "back" => return go_back(ctx, ui, screen),
        "delete" => return remove_focused(ctx, ui, screen),
        _ => return,
    };
    if !ui.get_focus_visible() {
        // The first key only shows where the focus is.
        ui.set_focus_visible(true);
        show_point(ui, focus_of(ctx, screen).point());
        return;
    }
    if screen == FocusScreen::Settings && direction == Direction::Down {
        // Settings uses the standard widgets: the side menu takes Up/Down/Enter, Tab the controls.
        ui.set_settings_menu_request(ui.get_settings_menu_request() + 1);
        return;
    }
    let rows = screen_rows(ui, screen);
    let nav = if screen == FocusScreen::Details {
        0
    } else {
        NAV_ITEMS
    };
    let point = with_grid(ctx, screen, |grid| grid.step(direction, &rows, nav));
    show_point(ui, point);
}

fn activate(ctx: &BridgeContext, ui: &CloudStream, screen: FocusScreen) {
    let point = focus_of(ctx, screen).point();
    if point.row == NAV_ROW {
        ui.invoke_select_tab(point.col);
        return;
    }
    let col = point.col;
    let index_in_grid = |first_row: i32| (point.row - first_row) * ui.get_grid_columns() + col;
    match screen {
        FocusScreen::Home => activate_home(ui, point),
        FocusScreen::Browse => ui.invoke_open_browse_hit(index_in_grid(0)),
        FocusScreen::Library => ui.invoke_open_library_hit(index_in_grid(0)),
        FocusScreen::Search if point.row == 0 => {
            ui.set_search_box_request(ui.get_search_box_request() + 1)
        }
        FocusScreen::Search => ui.invoke_open_search_hit(index_in_grid(1)),
        FocusScreen::Details => activate_details(ctx, ui, point),
        FocusScreen::Settings => {}
    }
}

fn activate_home(ui: &CloudStream, point: FocusPoint) {
    if point.row == ui.get_hero_row() {
        ui.invoke_resume_hero();
    } else if point.row == ui.get_continue_row() {
        ui.invoke_open_library_hit(point.col);
    } else {
        let shelf_offset = point.row - ui.get_shelf_row_base();
        let expanded = ui.global::<HomeExpanded>();
        let expanded_idx = expanded.get_shelf_index();
        if expanded_idx >= 0 && shelf_offset >= expanded_idx {
            let grid_row = shelf_offset - expanded_idx;
            expanded.invoke_open_item(grid_row * ui.get_grid_columns() + point.col);
        } else {
            ui.invoke_open_shelf_item(shelf_offset, point.col);
        }
    }
}

fn activate_details(ctx: &BridgeContext, ui: &CloudStream, point: FocusPoint) {
    let season_count = ui.get_season_labels().row_count();
    let seasons_row = if season_count > 1 && season_count <= MAX_SEASON_TABS {
        1
    } else {
        -1
    };
    let episodes_row = if seasons_row == 1 { 2 } else { 1 };
    if point.row == 0 {
        let action = DetailsButtons::of(ui)
            .order()
            .get(usize::try_from(point.col).unwrap_or(0))
            .copied();
        match action {
            Some(DetailsAction::Play) => ui.invoke_play_details(),
            Some(DetailsAction::Resume) => ui.invoke_resume_details(),
            Some(DetailsAction::Bookmark) => ui.invoke_toggle_bookmark(),
            Some(DetailsAction::Back) => close_details(ctx, ui),
            Some(DetailsAction::PlayNow) => ui.invoke_skip_checks(),
            None => {}
        }
    } else if point.row == seasons_row {
        if let Some(label) = ui
            .get_season_labels()
            .row_data(usize::try_from(point.col).unwrap_or(0))
        {
            ui.invoke_season_chosen(label);
        }
    } else {
        let cols = ui.get_episode_columns().max(1);
        let shown =
            usize::try_from((point.row - episodes_row) * cols + point.col).unwrap_or(usize::MAX);
        if let Some(episode) = ui.get_episodes().row_data(shown) {
            ui.invoke_play_episode(episode.index);
        }
    }
}

/// Delete on a focused title: out of Continue watching (Home) or out of the Library.
fn remove_focused(ctx: &BridgeContext, ui: &CloudStream, screen: FocusScreen) {
    let point = focus_of(ctx, screen).point();
    match screen {
        FocusScreen::Home if point.row == ui.get_continue_row() => {
            remove_continue(ctx, point.col);
        }
        FocusScreen::Library if point.row >= 0 => {
            remove_bookmark(ctx, point.row * ui.get_grid_columns() + point.col);
        }
        _ => {}
    }
}

fn go_back(ctx: &BridgeContext, ui: &CloudStream, screen: FocusScreen) {
    if screen == FocusScreen::Details {
        return close_details(ctx, ui);
    }
    if screen == FocusScreen::Home && ui.global::<HomeExpanded>().get_shelf_index() >= 0 {
        ui.global::<HomeExpanded>().invoke_close();
        return;
    }
    if screen == FocusScreen::Settings
        && ui.get_settings_page_index() == 2
        && ui.get_extensions_tab() == 0
        && !ui.global::<RepoDetail>().get_open_url().is_empty()
    {
        ui.global::<RepoDetail>().invoke_close();
        return;
    }
    let point = focus_of(ctx, screen).point();
    if point.row != NAV_ROW {
        let nav = FocusPoint {
            row: NAV_ROW,
            col: ui.get_tab(),
        };
        with_grid(ctx, screen, |grid| grid.set(nav));
        ui.set_focus_visible(true);
        show_point(ui, nav);
    } else if ui.get_tab() != 0 {
        ui.invoke_select_tab(0);
    }
}

fn close_details(ctx: &BridgeContext, ui: &CloudStream) {
    ctx.session.borrow_mut().focus.remove(&FocusScreen::Details);
    ui.invoke_close_details();
}

/// Shows the focus the current screen had (after a tab switch or a closed title page).
pub(crate) fn restore_focus(ctx: &BridgeContext, ui: &CloudStream) {
    ui.invoke_focus_main();
    if let Some(screen) = current_screen(ui) {
        show_point(ui, focus_of(ctx, screen).point());
    }
}

/// A title page opened: its focus starts on Play.
pub(crate) fn focus_details_start(ctx: &BridgeContext, ui: &CloudStream) {
    let start = FocusPoint { row: 0, col: 0 };
    with_grid(ctx, FocusScreen::Details, |grid| grid.set(start));
    ui.invoke_focus_main();
    show_point(ui, start);
}

fn focus_of(ctx: &BridgeContext, screen: FocusScreen) -> FocusGrid {
    ctx.session
        .borrow()
        .focus
        .get(&screen)
        .cloned()
        .unwrap_or_default()
}

fn with_grid<T>(
    ctx: &BridgeContext,
    screen: FocusScreen,
    change: impl FnOnce(&mut FocusGrid) -> T,
) -> T {
    let mut session = ctx.session.borrow_mut();
    change(session.focus.entry(screen).or_default())
}

fn show_point(ui: &CloudStream, point: FocusPoint) {
    ui.set_focus_row(point.row);
    ui.set_focus_col(point.col);
}
