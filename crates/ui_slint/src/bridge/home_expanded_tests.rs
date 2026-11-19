use core_model::{HomePage, HomePageSection, SearchResult};

use super::{expanded_count_text, pick_section_items};

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
