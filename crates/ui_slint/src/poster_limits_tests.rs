use super::{COLLAPSED_SHELF_POSTERS, EXPANDED_PAGE_SIZE};

#[test]
fn the_picture_limits_are_the_measured_ones() {
    assert_eq!(COLLAPSED_SHELF_POSTERS, 10);
    assert_eq!(EXPANDED_PAGE_SIZE, 20);
}
