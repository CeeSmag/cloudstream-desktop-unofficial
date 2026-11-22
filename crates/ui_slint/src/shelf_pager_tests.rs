use core_model::SearchResult;

use super::{collapsed_items, ShelfPager, COLLAPSED_SHELF_POSTERS, MAX_PAGER_ITEMS};

fn titles(count: usize, from: usize) -> Vec<SearchResult> {
    (from..from + count)
        .map(|number| SearchResult {
            name: format!("title {number}"),
            ..SearchResult::default()
        })
        .collect()
}

fn names(items: &[SearchResult]) -> Vec<&str> {
    items.iter().map(|item| item.name.as_str()).collect()
}

#[test]
fn page_slices_the_held_titles_into_pages_of_the_page_size() {
    let pager = ShelfPager::new(titles(50, 0), false, 20);
    assert_eq!(pager.page(1).map(<[_]>::len), Some(20));
    assert_eq!(pager.page(2).map(<[_]>::len), Some(20));
    assert_eq!(pager.page(3).map(<[_]>::len), Some(10));
    assert_eq!(names(pager.page(2).unwrap_or_default())[0], "title 20");
}

#[test]
fn page_zero_and_pages_past_the_end_are_none() {
    let pager = ShelfPager::new(titles(50, 0), false, 20);
    assert!(pager.page(0).is_none());
    assert!(pager.page(4).is_none());
}

#[test]
fn has_page_after_is_true_while_titles_remain_or_the_provider_has_more() {
    let held_only = ShelfPager::new(titles(50, 0), false, 20);
    assert!(held_only.has_page_after(1));
    assert!(held_only.has_page_after(2));
    assert!(!held_only.has_page_after(3));
    let provider_more = ShelfPager::new(titles(40, 0), true, 20);
    assert!(
        provider_more.has_page_after(2),
        "the provider may have a page 3"
    );
}

#[test]
fn needs_fetch_only_beyond_the_held_titles_and_when_the_provider_has_more() {
    let pager = ShelfPager::new(titles(40, 0), true, 20);
    assert!(!pager.needs_fetch_for(2), "page 2 is already held");
    assert!(pager.needs_fetch_for(3));
    let done = ShelfPager::new(titles(40, 0), false, 20);
    assert!(!done.needs_fetch_for(3), "nothing more to fetch");
}

#[test]
fn append_accepts_only_the_next_provider_page() {
    let mut pager = ShelfPager::new(titles(20, 0), true, 20);
    assert_eq!(pager.next_provider_page(), 2);
    assert!(
        !pager.append(3, titles(20, 20), true),
        "a skipped page is refused"
    );
    assert!(pager.append(2, titles(20, 20), true));
    assert!(
        !pager.append(2, titles(20, 40), true),
        "a repeated delivery is refused"
    );
    assert_eq!(pager.page(2).map(<[_]>::len), Some(20));
    assert!(
        pager.page(3).is_none(),
        "the refused delivery added nothing"
    );
    assert_eq!(pager.next_provider_page(), 3);
}

#[test]
fn an_empty_provider_page_ends_the_shelf() {
    let mut pager = ShelfPager::new(titles(20, 0), true, 20);
    assert!(pager.append(2, Vec::new(), true));
    assert!(!pager.has_page_after(1));
    assert!(!pager.needs_fetch_for(2));
}

#[test]
fn append_stops_at_the_cap_on_held_titles() {
    let mut pager = ShelfPager::new(titles(MAX_PAGER_ITEMS - 5, 0), true, 20);
    assert!(pager.append(2, titles(20, MAX_PAGER_ITEMS), true));
    assert_eq!(pager.held(), MAX_PAGER_ITEMS);
    assert!(!pager.needs_fetch_for(1_000), "the cap ends fetching");
}

#[test]
fn collapsed_items_keeps_only_the_first_posters_of_a_shelf() {
    let items = titles(50, 0);
    let shown = collapsed_items(&items);
    assert_eq!(shown.len(), COLLAPSED_SHELF_POSTERS);
    assert_eq!(names(shown)[0], "title 0");
    let short = titles(3, 0);
    assert_eq!(collapsed_items(&short).len(), 3);
}

#[test]
fn collapsed_row_is_a_prefix_of_the_first_expanded_page() {
    let pager = ShelfPager::new(titles(50, 0), false, 20);
    let first = pager.page(1).unwrap_or_default();
    let items = titles(50, 0);
    assert_eq!(
        names(collapsed_items(&items)),
        names(&first[..COLLAPSED_SHELF_POSTERS])
    );
}
