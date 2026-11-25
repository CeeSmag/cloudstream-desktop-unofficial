//! Keeps Home's place while the viewer is elsewhere.
//!
//! Home is rebuilt every time it is shown again (after a title page or another tab), so its
//! vertical scroll and each row's sideways scroll are kept in the `HomeScroll` global and put back
//! when it is created. Only the Home tab pressed while Home itself is on screen, or new Home
//! content, sends it back to the top.

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{CloudStream, HomeScroll};

/// The Home tab's index in the top bar.
const HOME_TAB: i32 = 0;

/// Rows whose sideways position is kept (Continue watching plus every shelf). Far above any real
/// provider's shelf count; a key beyond it is ignored instead of growing the list without bound.
pub(crate) const MAX_REMEMBERED_ROWS: usize = 512;

/// Whether Home itself is what the viewer sees: the Home tab with no title page over it.
pub(crate) fn is_home_on_screen(current_tab: i32, title_page_open: bool) -> bool {
    current_tab == HOME_TAB && !title_page_open
}

/// Whether choosing `chosen_tab` brings Home back to its top: only the Home tab, pressed while
/// Home itself is on screen. Coming back to Home from anywhere else keeps where it was.
pub(crate) fn is_home_top_request(home_on_screen: bool, chosen_tab: i32) -> bool {
    home_on_screen && chosen_tab == HOME_TAB
}

/// Stores row `row`'s sideways offset `x`; rows in between start at 0. Returns false (and leaves
/// the list unchanged) for a negative row, a row past [`MAX_REMEMBERED_ROWS`] or an `x` that is
/// not a number.
pub(crate) fn store_row_offset(model: &VecModel<f32>, row: i32, x: f32) -> bool {
    let index = match usize::try_from(row) {
        Ok(index) if index < MAX_REMEMBERED_ROWS && x.is_finite() => index,
        _ => {
            tracing::debug!(row, x, "home row offset not kept");
            return false;
        }
    };
    while model.row_count() <= index {
        model.push(0.0);
    }
    model.set_row_data(index, x);
    true
}

/// Gives `HomeScroll` its list of row offsets and remembers every sideways scroll of a Home row.
pub fn wire_home_scroll(ui: &CloudStream) {
    let scroll = ui.global::<HomeScroll>();
    scroll.set_row_x(ModelRc::new(VecModel::<f32>::default()));
    let weak = ui.as_weak();
    scroll.on_row_scrolled(move |row, x| {
        if let Some(ui) = weak.upgrade() {
            remember_row(&ui, row, x);
        }
    });
}

/// Sends Home back to its top and every row back to its first poster (on screen now, or the next
/// time Home is shown).
pub fn reset_home_scroll(ui: &CloudStream) {
    let scroll = ui.global::<HomeScroll>();
    tracing::debug!(page_y = scroll.get_page_y(), "home scroll reset to the top");
    scroll.set_page_y(0.0);
    let rows = scroll.get_row_x();
    match rows.as_any().downcast_ref::<VecModel<f32>>() {
        Some(model) => model.set_vec(Vec::new()),
        None => scroll.set_row_x(ModelRc::new(VecModel::<f32>::default())),
    }
    scroll.set_top_request(scroll.get_top_request().wrapping_add(1));
}

fn remember_row(ui: &CloudStream, row: i32, x: f32) {
    let scroll = ui.global::<HomeScroll>();
    let rows = scroll.get_row_x();
    // A refused offset is logged by `store_row_offset`; the row simply starts at 0 next time.
    match rows.as_any().downcast_ref::<VecModel<f32>>() {
        Some(model) => {
            store_row_offset(model, row, x);
        }
        None => {
            // Not wired yet (or replaced from outside): start a list this module owns.
            let model = VecModel::<f32>::default();
            store_row_offset(&model, row, x);
            scroll.set_row_x(ModelRc::new(model));
        }
    }
}

#[cfg(test)]
#[path = "home_scroll_tests.rs"]
mod tests;
