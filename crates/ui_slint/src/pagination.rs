//! Numbered pagination slot computation for Browse (e.g. `1 2 3 4 5 … 167`).

use slint::{ModelRc, VecModel};

use crate::PaginationItem;

/// Maximum page count rendered without any ellipsis.
const MAX_INLINE_PAGES: u32 = 7;

/// One slot in a numbered pagination bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageSlot {
    Page(u32),
    Ellipsis,
}

/// Computes the balanced sequence of page buttons and ellipses for `current` out of `total` pages.
///
/// Returns an empty list when `total <= 1`. For `total > 7`, always returns 7 slots so the
/// pagination bar width stays visually steady across page changes.
pub(crate) fn page_slots(current: u32, total: u32) -> Vec<PageSlot> {
    if total <= 1 {
        return Vec::new();
    }
    if total <= MAX_INLINE_PAGES {
        return (1..=total).map(PageSlot::Page).collect();
    }
    let page = current.clamp(1, total);
    if page <= 4 {
        let mut slots: Vec<PageSlot> = (1..=5).map(PageSlot::Page).collect();
        slots.push(PageSlot::Ellipsis);
        slots.push(PageSlot::Page(total));
        return slots;
    }
    if page >= total - 3 {
        let mut slots = vec![PageSlot::Page(1), PageSlot::Ellipsis];
        slots.extend((total - 4..=total).map(PageSlot::Page));
        return slots;
    }
    vec![
        PageSlot::Page(1),
        PageSlot::Ellipsis,
        PageSlot::Page(page - 1),
        PageSlot::Page(page),
        PageSlot::Page(page + 1),
        PageSlot::Ellipsis,
        PageSlot::Page(total),
    ]
}

/// Converts [`page_slots`] into the Slint model read by `PaginationBar`.
pub(crate) fn pagination_model(current: u32, total: u32) -> ModelRc<PaginationItem> {
    let active_page = current.clamp(1, total.max(1));
    let items: Vec<PaginationItem> = page_slots(active_page, total)
        .into_iter()
        .map(|slot| match slot {
            PageSlot::Page(number) => PaginationItem {
                label: number.to_string().into(),
                page: i32::try_from(number).unwrap_or(i32::MAX),
                active: number == active_page,
                ellipsis: false,
            },
            PageSlot::Ellipsis => PaginationItem {
                label: "…".into(),
                page: 0,
                active: false,
                ellipsis: true,
            },
        })
        .collect();
    ModelRc::new(VecModel::from(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(current: u32, total: u32) -> Vec<String> {
        page_slots(current, total)
            .into_iter()
            .map(|slot| match slot {
                PageSlot::Page(n) => n.to_string(),
                PageSlot::Ellipsis => "…".to_owned(),
            })
            .collect()
    }

    #[test]
    fn empty_for_zero_or_single_page() {
        assert!(page_slots(1, 0).is_empty());
        assert!(page_slots(1, 1).is_empty());
    }

    #[test]
    fn lists_all_pages_up_to_seven() {
        assert_eq!(labels(1, 2), ["1", "2"]);
        assert_eq!(labels(4, 7), ["1", "2", "3", "4", "5", "6", "7"]);
    }

    #[test]
    fn keeps_seven_slots_at_start_middle_and_end() {
        assert_eq!(labels(1, 167), ["1", "2", "3", "4", "5", "…", "167"]);
        assert_eq!(labels(4, 167), ["1", "2", "3", "4", "5", "…", "167"]);
        assert_eq!(labels(5, 167), ["1", "…", "4", "5", "6", "…", "167"]);
        assert_eq!(labels(84, 167), ["1", "…", "83", "84", "85", "…", "167"]);
        assert_eq!(
            labels(163, 167),
            ["1", "…", "162", "163", "164", "…", "167"]
        );
        assert_eq!(
            labels(164, 167),
            ["1", "…", "163", "164", "165", "166", "167"]
        );
        assert_eq!(
            labels(167, 167),
            ["1", "…", "163", "164", "165", "166", "167"]
        );
    }

    #[test]
    fn clamps_out_of_range_current_page() {
        assert_eq!(labels(0, 20), ["1", "2", "3", "4", "5", "…", "20"]);
        assert_eq!(labels(999, 20), ["1", "…", "16", "17", "18", "19", "20"]);
    }
}
