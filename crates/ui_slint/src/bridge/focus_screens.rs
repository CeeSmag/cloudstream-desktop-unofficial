//! The rows each screen offers to the remote, and what its title-page buttons are.

use slint::{ComponentHandle, Model};

use crate::focus_grid::grid_rows;
use crate::CloudStream;

/// The screens with their own focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FocusScreen {
    Home,
    Browse,
    Search,
    Library,
    Settings,
    Details,
}

/// Page padding plus the grid's own padding and the scroll bar, in logical pixels.
const GRID_CHROME: f32 = 2.0 * 36.0 + 16.0 + 14.0;
/// One tile and its gap.
const TILE_STEP: f32 = 172.0;
/// Below this the top bar drops the long logo and the provider chip.
const NARROW_WIDTH: f32 = 980.0;
/// Above this the top bar also shows how the opening goes.
const WIDE_WIDTH: f32 = 1200.0;

/// The screen the keys act on; None while the player is open (it has its own keys).
pub(crate) fn current_screen(ui: &CloudStream) -> Option<FocusScreen> {
    if ui.get_player_open() || ui.get_starting() {
        return None;
    }
    if !ui.get_details_title().is_empty() {
        return Some(FocusScreen::Details);
    }
    Some(match ui.get_tab() {
        0 => FocusScreen::Home,
        1 => FocusScreen::Browse,
        2 => FocusScreen::Search,
        3 => FocusScreen::Library,
        _ => FocusScreen::Settings,
    })
}

/// The window's width in logical pixels.
pub(crate) fn logical_width(ui: &CloudStream) -> f32 {
    let window = ui.window();
    window.size().width as f32 / window.scale_factor().max(0.5)
}

/// Follows the window's width: grid columns and the top bar's narrow / wide layout.
pub(crate) fn follow_window_width(ui: &CloudStream) {
    let width = logical_width(ui);
    let columns = columns_for_width(width);
    if ui.get_grid_columns() != columns {
        ui.set_grid_columns(columns);
    }
    ui.set_narrow_window(width < NARROW_WIDTH);
    ui.set_wide_window(width > WIDE_WIDTH);
}

pub(crate) fn columns_for_width(logical_width: f32) -> i32 {
    (((logical_width - GRID_CHROME + 12.0) / TILE_STEP).floor() as i32).max(1)
}

/// The item counts of `screen`'s rows (Home also tells the window where its rows are).
pub(crate) fn screen_rows(ui: &CloudStream, screen: FocusScreen) -> Vec<usize> {
    let columns = usize::try_from(ui.get_grid_columns()).unwrap_or(1);
    match screen {
        FocusScreen::Home => home_rows(ui),
        FocusScreen::Browse => grid_rows(ui.get_browse_items().row_count(), columns),
        FocusScreen::Library => grid_rows(ui.get_library_items().row_count(), columns),
        FocusScreen::Search => {
            let mut rows = vec![1];
            rows.extend(grid_rows(ui.get_search_items().row_count(), columns));
            rows
        }
        FocusScreen::Settings => Vec::new(),
        FocusScreen::Details => details_rows(ui),
    }
}

fn home_rows(ui: &CloudStream) -> Vec<usize> {
    let mut rows = Vec::new();
    let continue_count = ui.get_continue_items().row_count();
    if continue_count > 0 {
        ui.set_hero_row(0);
        ui.set_continue_row(1);
        rows.extend([1, continue_count]);
    } else {
        ui.set_hero_row(-1);
        ui.set_continue_row(-1);
    }
    ui.set_shelf_row_base(i32::try_from(rows.len()).unwrap_or(0));
    rows.extend(
        ui.get_home_shelves()
            .iter()
            .map(|shelf| shelf.items.row_count()),
    );
    rows
}

fn details_rows(ui: &CloudStream) -> Vec<usize> {
    let buttons = DetailsButtons::of(ui);
    let mut rows = vec![buttons.count()];
    let seasons = ui.get_season_labels().row_count();
    if seasons > 1 {
        rows.push(seasons);
    }
    rows.extend(std::iter::repeat_n(1, ui.get_episodes().row_count()));
    rows
}

/// A button of the title page's first row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailsAction {
    Play,
    Resume,
    Bookmark,
    Back,
    PlayNow,
}

/// Which buttons the title page shows (the same rules as `details.slint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DetailsButtons {
    pub(crate) loading: bool,
    pub(crate) has_resume: bool,
    pub(crate) opening: bool,
}

impl DetailsButtons {
    pub(crate) fn of(ui: &CloudStream) -> Self {
        Self {
            loading: ui.get_details_loading(),
            has_resume: !ui.get_resume_label().is_empty(),
            opening: ui.get_open_busy(),
        }
    }

    /// The buttons in their order on screen.
    pub(crate) fn order(self) -> Vec<DetailsAction> {
        if self.loading {
            return vec![DetailsAction::Back];
        }
        let mut order = vec![DetailsAction::Play];
        if self.has_resume {
            order.push(DetailsAction::Resume);
        }
        order.extend([DetailsAction::Bookmark, DetailsAction::Back]);
        if self.opening {
            order.push(DetailsAction::PlayNow);
        }
        order
    }

    pub(crate) fn count(self) -> usize {
        self.order().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_buttons_order_matches_the_page() {
        let plain = DetailsButtons {
            loading: false,
            has_resume: false,
            opening: false,
        };
        assert_eq!(
            plain.order(),
            [
                DetailsAction::Play,
                DetailsAction::Bookmark,
                DetailsAction::Back
            ]
        );
        let full = DetailsButtons {
            loading: false,
            has_resume: true,
            opening: true,
        };
        assert_eq!(
            full.order(),
            [
                DetailsAction::Play,
                DetailsAction::Resume,
                DetailsAction::Bookmark,
                DetailsAction::Back,
                DetailsAction::PlayNow
            ]
        );
        let loading = DetailsButtons {
            loading: true,
            has_resume: true,
            opening: false,
        };
        assert_eq!(loading.order(), [DetailsAction::Back]);
    }

    #[test]
    fn columns_for_width_fits_whole_tiles() {
        assert_eq!(columns_for_width(760.0), 3);
        assert_eq!(columns_for_width(1920.0), 10);
        assert_eq!(columns_for_width(100.0), 1);
    }
}
