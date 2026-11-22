use slint::{Rgba8Pixel, SharedPixelBuffer};

use super::ImageStore;
use crate::ui_events::PosterKind;

/// A buffer of `side * side * 4` bytes.
fn buffer(side: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    SharedPixelBuffer::<Rgba8Pixel>::new(side, side)
}

/// 5x5 pixels = 100 bytes.
fn hundred() -> SharedPixelBuffer<Rgba8Pixel> {
    buffer(5)
}

fn same_pixels(a: &SharedPixelBuffer<Rgba8Pixel>, b: &SharedPixelBuffer<Rgba8Pixel>) -> bool {
    std::ptr::eq(a.as_bytes().as_ptr(), b.as_bytes().as_ptr())
}

#[test]
fn an_address_stored_twice_shares_one_buffer() {
    let mut store = ImageStore::new(1_000);
    let first = store.insert("a", hundred());
    let second = store.insert("a", hundred());
    assert!(
        same_pixels(&first, &second),
        "the second insert returned the first buffer"
    );
    assert_eq!(store.len(), 1);
    assert_eq!(store.used_bytes(), 100);
}

#[test]
fn get_returns_what_was_stored_and_nothing_for_an_unknown_address() {
    let mut store = ImageStore::new(1_000);
    let stored = store.insert("a", hundred());
    assert!(same_pixels(&store.get("a").expect("stored"), &stored));
    assert!(store.get("b").is_none());
}

#[test]
fn unpinned_pictures_are_evicted_oldest_first_when_over_budget() {
    let mut store = ImageStore::new(250);
    store.insert("a", hundred());
    store.insert("b", hundred());
    store.insert("c", hundred());
    assert!(store.get("a").is_none(), "the oldest went first");
    assert!(store.get("b").is_some());
    assert!(store.get("c").is_some());
    assert!(store.used_bytes() <= 250);
}

#[test]
fn reading_a_picture_keeps_it_from_being_the_oldest() {
    let mut store = ImageStore::new(250);
    store.insert("a", hundred());
    store.insert("b", hundred());
    let _ = store.get("a");
    store.insert("c", hundred());
    assert!(store.get("a").is_some(), "a was used more recently than b");
    assert!(store.get("b").is_none());
}

#[test]
fn pinned_pictures_are_never_evicted_even_over_budget() {
    let mut store = ImageStore::new(150);
    store.set_pins(PosterKind::Shelf, ["a".to_owned(), "b".to_owned()]);
    store.insert("a", hundred());
    store.insert("b", hundred());
    assert!(store.get("a").is_some());
    assert!(store.get("b").is_some());
    assert_eq!(
        store.used_bytes(),
        200,
        "over budget, but both are on screen"
    );
}

#[test]
fn releasing_a_pin_makes_the_picture_evictable_at_once() {
    let mut store = ImageStore::new(150);
    store.set_pins(PosterKind::Shelf, ["a".to_owned(), "b".to_owned()]);
    store.insert("a", hundred());
    store.insert("b", hundred());
    store.set_pins(PosterKind::Shelf, ["b".to_owned()]);
    assert!(
        store.get("a").is_none(),
        "a left the screen and the store was over budget"
    );
    assert!(store.get("b").is_some());
}

#[test]
fn a_picture_stays_while_any_owner_pins_it() {
    let mut store = ImageStore::new(50);
    store.set_pins(PosterKind::Shelf, ["a".to_owned()]);
    store.set_pins(PosterKind::HomeExpanded, ["a".to_owned()]);
    store.insert("a", hundred());
    store.set_pins(PosterKind::Shelf, []);
    assert!(store.get("a").is_some(), "the expanded view still shows it");
    store.set_pins(PosterKind::HomeExpanded, []);
    assert!(store.get("a").is_none());
}

#[test]
fn an_owner_replaces_its_pins_it_does_not_add_to_them() {
    let mut store = ImageStore::new(1_000);
    store.set_pins(PosterKind::Search, ["a".to_owned()]);
    store.set_pins(PosterKind::Search, ["b".to_owned()]);
    store.insert("a", hundred());
    store.insert("b", hundred());
    assert!(!store.is_pinned("a"));
    assert!(store.is_pinned("b"));
}

#[test]
fn unpinned_pictures_inside_the_budget_are_kept_for_a_quick_return() {
    let mut store = ImageStore::new(1_000);
    store.set_pins(PosterKind::HomeExpanded, ["a".to_owned()]);
    store.insert("a", hundred());
    store.set_pins(PosterKind::HomeExpanded, []);
    assert!(
        store.get("a").is_some(),
        "collapsed, but within budget: pressing expand again is instant"
    );
}
