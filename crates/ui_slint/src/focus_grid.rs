//! Where the remote / keyboard focus is and where an arrow key moves it.
//!
//! A screen is a list of rows, each with some number of items (Home: the hero button, the
//! continue-watching row, one row per shelf; Browse: the grid's rows; a title page: its buttons,
//! then one row per episode). Row [`NAV_ROW`] is the top bar. Up and Down keep the column each row
//! had last (so going down a column of shelves and back up lands on the same posters), Left and
//! Right stop at the ends.

use std::collections::HashMap;

/// The top bar's row.
pub(crate) const NAV_ROW: i32 = -1;

/// An arrow key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// A focused item: its row and its place in the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusPoint {
    pub(crate) row: i32,
    pub(crate) col: i32,
}

/// The focus of one screen.
#[derive(Debug, Clone)]
pub(crate) struct FocusGrid {
    point: FocusPoint,
    /// The column each row had when the focus last left it.
    remembered: HashMap<i32, i32>,
}

impl Default for FocusGrid {
    fn default() -> Self {
        Self {
            point: FocusPoint { row: 0, col: 0 },
            remembered: HashMap::new(),
        }
    }
}

impl FocusGrid {
    /// The focused item.
    pub(crate) fn point(&self) -> FocusPoint {
        self.point
    }

    /// Moves the focus one step; `rows[i]` is the number of items of row `i`, `nav_items` those of
    /// the top bar. Rows without items are skipped.
    pub(crate) fn step(
        &mut self,
        direction: Direction,
        rows: &[usize],
        nav_items: usize,
    ) -> FocusPoint {
        self.point = self.clamped(rows, nav_items);
        let next = match direction {
            Direction::Left => self.sideways(-1, rows, nav_items),
            Direction::Right => self.sideways(1, rows, nav_items),
            Direction::Up => self.vertical(-1, rows, nav_items),
            Direction::Down => self.vertical(1, rows, nav_items),
        };
        if next.row != self.point.row {
            self.remembered.insert(self.point.row, self.point.col);
        }
        self.point = next;
        next
    }

    /// Puts the focus on `point` (a click, or a screen that opens on a given item).
    pub(crate) fn set(&mut self, point: FocusPoint) {
        self.point = point;
    }

    fn sideways(&self, delta: i32, rows: &[usize], nav_items: usize) -> FocusPoint {
        let count = items_in(self.point.row, rows, nav_items);
        let col = (self.point.col + delta).clamp(0, count.saturating_sub(1) as i32);
        FocusPoint {
            row: self.point.row,
            col,
        }
    }

    fn vertical(&self, delta: i32, rows: &[usize], nav_items: usize) -> FocusPoint {
        let mut row = self.point.row + delta;
        while row >= 0 && (row as usize) < rows.len() && items_in(row, rows, nav_items) == 0 {
            row += delta;
        }
        let last = rows.len() as i32 - 1;
        if row < NAV_ROW || row > last || (row != NAV_ROW && items_in(row, rows, nav_items) == 0) {
            return self.point;
        }
        let wanted = self.remembered.get(&row).copied().unwrap_or(self.point.col);
        let count = items_in(row, rows, nav_items);
        FocusPoint {
            row,
            col: wanted.clamp(0, count.saturating_sub(1) as i32),
        }
    }

    /// The focus moved back inside the screen when rows shrank (a shelf reloaded with fewer items).
    fn clamped(&self, rows: &[usize], nav_items: usize) -> FocusPoint {
        let last = rows.len() as i32 - 1;
        let row = if self.point.row > last {
            last.max(NAV_ROW)
        } else {
            self.point.row
        };
        let count = items_in(row, rows, nav_items);
        FocusPoint {
            row,
            col: self.point.col.clamp(0, count.saturating_sub(1) as i32),
        }
    }
}

fn items_in(row: i32, rows: &[usize], nav_items: usize) -> usize {
    if row == NAV_ROW {
        return nav_items;
    }
    usize::try_from(row)
        .ok()
        .and_then(|index| rows.get(index))
        .copied()
        .unwrap_or(0)
}

/// The rows of a wrapping grid of `items` with `columns` per row.
pub(crate) fn grid_rows(items: usize, columns: usize) -> Vec<usize> {
    let columns = columns.max(1);
    (0..items.div_ceil(columns))
        .map(|row| (items - row * columns).min(columns))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAV: usize = 5;

    #[test]
    fn step_right_stops_at_row_end_left_at_start() {
        let mut grid = FocusGrid::default();
        let rows = [3];
        assert_eq!(grid.step(Direction::Right, &rows, NAV).col, 1);
        assert_eq!(grid.step(Direction::Right, &rows, NAV).col, 2);
        assert_eq!(grid.step(Direction::Right, &rows, NAV).col, 2);
        grid.set(FocusPoint { row: 0, col: 0 });
        assert_eq!(grid.step(Direction::Left, &rows, NAV).col, 0);
    }

    #[test]
    fn step_up_from_first_row_reaches_the_top_bar_and_comes_back() {
        let mut grid = FocusGrid::default();
        let rows = [1, 10];
        grid.set(FocusPoint { row: 1, col: 6 });
        assert_eq!(
            grid.step(Direction::Up, &rows, NAV),
            FocusPoint { row: 0, col: 0 }
        );
        assert_eq!(
            grid.step(Direction::Up, &rows, NAV),
            FocusPoint {
                row: NAV_ROW,
                col: 0
            }
        );
        assert_eq!(
            grid.step(Direction::Up, &rows, NAV).row,
            NAV_ROW,
            "nothing above the bar"
        );
        grid.step(Direction::Down, &rows, NAV);
        assert_eq!(
            grid.step(Direction::Down, &rows, NAV),
            FocusPoint { row: 1, col: 6 }
        );
    }

    #[test]
    fn step_down_remembers_each_rows_column() {
        let mut grid = FocusGrid::default();
        let rows = [8, 8, 8];
        grid.set(FocusPoint { row: 0, col: 4 });
        grid.step(Direction::Down, &rows, NAV);
        grid.step(Direction::Left, &rows, NAV);
        grid.step(Direction::Left, &rows, NAV);
        assert_eq!(grid.point(), FocusPoint { row: 1, col: 2 });
        grid.step(Direction::Up, &rows, NAV);
        assert_eq!(
            grid.point(),
            FocusPoint { row: 0, col: 4 },
            "back where row 0 was left"
        );
    }

    #[test]
    fn step_skips_empty_rows_and_clamps_to_shorter_rows() {
        let mut grid = FocusGrid::default();
        let rows = [6, 0, 2];
        grid.set(FocusPoint { row: 0, col: 5 });
        assert_eq!(
            grid.step(Direction::Down, &rows, NAV),
            FocusPoint { row: 2, col: 1 }
        );
        assert_eq!(
            grid.step(Direction::Down, &rows, NAV).row,
            2,
            "nothing below the last row"
        );
    }

    #[test]
    fn step_after_rows_shrank_starts_from_a_valid_item() {
        let mut grid = FocusGrid::default();
        grid.set(FocusPoint { row: 4, col: 9 });
        assert_eq!(
            grid.step(Direction::Left, &[3, 2], NAV),
            FocusPoint { row: 1, col: 0 }
        );
    }

    #[test]
    fn grid_rows_splits_items_into_columns() {
        assert_eq!(grid_rows(7, 3), vec![3, 3, 1]);
        assert_eq!(grid_rows(0, 3), Vec::<usize>::new());
        assert_eq!(grid_rows(2, 0), vec![1, 1]);
    }
}
