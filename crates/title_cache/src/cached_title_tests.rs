use super::*;
use crate::test_folder::{cached, link, title};

fn urls(title: &CachedTitle) -> Vec<&str> {
    title
        .playing_order()
        .into_iter()
        .map(|cached| cached.link.url.as_str())
        .collect()
}

#[test]
fn working_links_come_first_then_unchecked_then_broken_each_by_quality() {
    let title = title(vec![
        cached("broken-1080", 1080, LinkHealth::Broken),
        cached("unchecked-720", 720, LinkHealth::Unchecked),
        cached("working-480", 480, LinkHealth::Working),
        cached("working-720", 720, LinkHealth::Working),
        cached("unchecked-1080", 1080, LinkHealth::Unchecked),
    ]);
    assert_eq!(
        urls(&title),
        [
            "working-720",
            "working-480",
            "unchecked-1080",
            "unchecked-720",
            "broken-1080"
        ]
    );
}

#[test]
fn the_link_that_played_well_last_time_goes_first_even_against_better_quality() {
    let mut title = title(vec![
        cached("working-1080", 1080, LinkHealth::Working),
        cached("played-480", 480, LinkHealth::Working),
    ]);
    title.last_good = Some("played-480".to_owned());
    assert_eq!(urls(&title), ["played-480", "working-1080"]);
}

#[test]
fn a_last_good_link_that_was_found_broken_since_is_not_put_first() {
    let mut title = title(vec![
        cached("working-720", 720, LinkHealth::Working),
        cached("was-good-1080", 1080, LinkHealth::Broken),
    ]);
    title.last_good = Some("was-good-1080".to_owned());
    assert_eq!(urls(&title), ["working-720", "was-good-1080"]);
}

#[test]
fn the_same_address_appears_once_and_the_first_copy_counts() {
    let title = title(vec![
        cached("same", 480, LinkHealth::Working),
        cached("same", 1080, LinkHealth::Broken),
        cached("other", 720, LinkHealth::Working),
    ]);
    assert_eq!(urls(&title), ["other", "same"]);
    assert_eq!(title.playing_order()[1].link.quality, 480);
}

#[test]
fn normalizing_keeps_the_best_links_and_forgets_a_last_good_that_was_cut() {
    let mut title = title(
        (0..10)
            .map(|n| cached(&format!("u{n}"), n, LinkHealth::Unchecked))
            .collect(),
    );
    title.last_good = Some("u0".to_owned());
    let kept = title.normalized(3);
    assert_eq!(kept.links.len(), 3);
    assert!(kept.links.iter().any(|cached| cached.link.url == "u0"));

    let mut title = self::title(
        (0..10)
            .map(|n| cached(&format!("u{n}"), n, LinkHealth::Unchecked))
            .collect(),
    );
    title.last_good = Some("u0".to_owned());
    title.set_health("u0", LinkHealth::Broken);
    let kept = title.normalized(3);
    assert_eq!(kept.last_good, None);
    assert!(!kept.links.iter().any(|cached| cached.link.url == "u0"));
}

#[test]
fn marking_a_link_broken_clears_it_as_last_good_and_unknown_links_are_ignored() {
    let mut title = title(vec![cached("a", 720, LinkHealth::Working)]);
    title.set_played_well("a");
    assert_eq!(title.last_good.as_deref(), Some("a"));
    title.set_health("missing", LinkHealth::Broken);
    title.set_played_well("missing");
    assert_eq!(title.last_good.as_deref(), Some("a"));
    title.set_health("a", LinkHealth::Broken);
    assert_eq!(title.last_good, None);
    assert_eq!(title.links[0].health, LinkHealth::Broken);
}

#[test]
fn a_link_missing_its_health_in_the_file_counts_as_unchecked() {
    let json = serde_json::to_value(link("u", 1)).expect("encode");
    let read: CachedLink =
        serde_json::from_value(serde_json::json!({ "link": json })).expect("decode");
    assert_eq!(read.health, LinkHealth::Unchecked);
}
