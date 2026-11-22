use core_model::{HomePage, HomePageSection, SearchResult};

use crate::shelf_pager::ShelfPager;

use super::{accept_loaded_page, expanded_count_text, pick_section_items};
use super::{ExpandedShelfState, LoadedPage};

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

fn results(count: usize, from: usize) -> Vec<SearchResult> {
    (from..from + count)
        .map(|number| SearchResult {
            name: format!("title {number}"),
            ..SearchResult::default()
        })
        .collect()
}

fn state_waiting_for(page: u32) -> ExpandedShelfState {
    ExpandedShelfState {
        shelf_index: 0,
        page: 2,
        awaiting: Some(page),
        pager: ShelfPager::new(results(40, 0), true, 20),
    }
}

#[test]
fn accept_loaded_page_shows_the_page_that_was_waited_for() {
    let mut state = state_waiting_for(3);
    match accept_loaded_page(&mut state, 2, results(20, 40), true) {
        LoadedPage::Show(items, meta) => {
            assert_eq!(items.len(), 20);
            assert_eq!(items[0].name, "title 40");
            assert_eq!((meta.page, meta.has_next), (3, true));
        }
        _ => panic!("expected the loaded page to be shown"),
    }
    assert_eq!(state.page, 3);
    assert_eq!(state.awaiting, None);
}

#[test]
fn accept_loaded_page_ignores_a_delivery_nobody_waits_for() {
    let mut state = state_waiting_for(3);
    state.awaiting = None;
    assert!(matches!(
        accept_loaded_page(&mut state, 2, results(20, 40), true),
        LoadedPage::Ignored
    ));
    assert_eq!(state.pager.page(3), None, "nothing was appended");
}

#[test]
fn accept_loaded_page_ignores_a_repeated_delivery() {
    let mut state = state_waiting_for(3);
    assert!(matches!(
        accept_loaded_page(&mut state, 2, results(20, 40), true),
        LoadedPage::Show(..)
    ));
    state.awaiting = Some(4);
    assert!(matches!(
        accept_loaded_page(&mut state, 2, results(20, 60), true),
        LoadedPage::Ignored
    ));
    assert_eq!(state.pager.page(4), None);
}

#[test]
fn accept_loaded_page_reports_the_end_when_the_provider_sends_nothing() {
    let mut state = state_waiting_for(3);
    assert!(matches!(
        accept_loaded_page(&mut state, 2, Vec::new(), false),
        LoadedPage::End
    ));
    assert_eq!(state.page, 2, "the viewer stays on the last real page");
    assert_eq!(state.awaiting, None);
}
