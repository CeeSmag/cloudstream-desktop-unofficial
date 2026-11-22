use super::{COLLAPSED_SHELF_POSTERS, EXPANDED_PAGE_SIZE, IMAGE_STORE_BYTES};

#[test]
fn the_picture_limits_are_the_measured_ones() {
    assert_eq!(COLLAPSED_SHELF_POSTERS, 10);
    assert_eq!(EXPANDED_PAGE_SIZE, 20);
}

#[test]
fn the_picture_store_keeps_64_mib() {
    assert_eq!(IMAGE_STORE_BYTES, 64 * 1024 * 1024);
}
