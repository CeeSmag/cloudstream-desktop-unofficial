//! How many of a Home shelf's titles are on screen at once, and how the expanded view pages them.
//!
//! Memory rule: the pictures held in memory follow what the viewer can see, not how many titles a
//! provider returned. A collapsed shelf shows its first [`COLLAPSED_SHELF_POSTERS`]; an expanded
//! shelf shows one page of `EXPANDED_PAGE_SIZE` and Next/Prev replace it. The titles' text data
//! (about 1 KB each) is kept in a [`ShelfPager`] so paging never asks the provider again for what
//! it already sent; only the pictures (about 0.8 MB each when decoded) are bounded.

use core_model::SearchResult;

use crate::poster_limits::COLLAPSED_SHELF_POSTERS;

/// Most titles' text data one expanded shelf keeps while the viewer pages through provider pages.
pub(crate) const MAX_PAGER_ITEMS: usize = 1_000;

/// The titles of a collapsed shelf row: a prefix of the first expanded page, so expanding reuses
/// the pictures already loaded.
pub(crate) fn collapsed_items(items: &[SearchResult]) -> &[SearchResult] {
    items.get(..COLLAPSED_SHELF_POSTERS).unwrap_or(items)
}

/// One expanded shelf: every title received so far, cut into pages of `page_size`.
///
/// UI pages and provider pages are independent: a provider may send 50 titles at once and the UI
/// shows them 20 at a time, asking the provider for its next page only when they run out.
#[derive(Debug, Clone)]
pub(crate) struct ShelfPager {
    items: Vec<SearchResult>,
    page_size: usize,
    provider_pages: u32,
    provider_has_next: bool,
}

impl ShelfPager {
    /// `items` is the provider's first page (what Home already loaded).
    pub(crate) fn new(items: Vec<SearchResult>, provider_has_next: bool, page_size: usize) -> Self {
        Self {
            items,
            page_size: page_size.max(1),
            provider_pages: 1,
            provider_has_next,
        }
    }

    /// Titles of UI page `page` (1-based); `None` when the page is empty or not loaded yet.
    pub(crate) fn page(&self, page: u32) -> Option<&[SearchResult]> {
        let start = usize::try_from(page.checked_sub(1)?)
            .ok()?
            .checked_mul(self.page_size)?;
        let end = start.saturating_add(self.page_size).min(self.items.len());
        if start >= end {
            return None;
        }
        self.items.get(start..end)
    }

    /// Whether a page after `page` exists or may exist at the provider.
    pub(crate) fn has_page_after(&self, page: u32) -> bool {
        self.held_through(page) < self.items.len() || self.provider_has_next
    }

    /// True when `page` lies beyond the titles held and the provider can still send more.
    pub(crate) fn needs_fetch_for(&self, page: u32) -> bool {
        let first_wanted = self.held_through(page.saturating_sub(1));
        first_wanted >= self.items.len() && self.provider_has_next
    }

    /// The provider page to ask for next (the first page came with the shelf).
    pub(crate) fn next_provider_page(&self) -> u32 {
        self.provider_pages.saturating_add(1)
    }

    /// Adds a provider page. Returns `false`, changing nothing, unless it is the page that was
    /// asked for (a late or repeated delivery must not be appended twice).
    pub(crate) fn append(
        &mut self,
        provider_page: u32,
        items: Vec<SearchResult>,
        has_next: bool,
    ) -> bool {
        if provider_page != self.next_provider_page() {
            return false;
        }
        let room = MAX_PAGER_ITEMS.saturating_sub(self.items.len());
        let sent_anything = !items.is_empty();
        let more_than_room = items.len() > room;
        self.items.extend(items.into_iter().take(room));
        self.provider_pages = provider_page;
        // An empty page ends the shelf even if the provider still says "has next".
        self.provider_has_next =
            has_next && sent_anything && !more_than_room && self.items.len() < MAX_PAGER_ITEMS;
        true
    }

    /// How many titles are held.
    #[cfg(test)]
    pub(crate) fn held(&self) -> usize {
        self.items.len()
    }

    /// Number of titles on pages 1 through `page`.
    fn held_through(&self, page: u32) -> usize {
        usize::try_from(page)
            .unwrap_or(usize::MAX)
            .saturating_mul(self.page_size)
    }
}

#[cfg(test)]
#[path = "shelf_pager_tests.rs"]
mod tests;
