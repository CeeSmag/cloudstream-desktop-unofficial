use super::{
    is_offered_read_ahead, is_offered_storage_gb, is_offered_video_cache,
    READ_AHEAD_CHOICES_PERCENT, STORAGE_LIMIT_CHOICES_GB, VIDEO_CACHE_CHOICES_MIB,
};

#[test]
fn video_cache_offers_64_mb_to_4_gb() {
    assert_eq!(VIDEO_CACHE_CHOICES_MIB.first(), Some(&64));
    assert_eq!(VIDEO_CACHE_CHOICES_MIB.last(), Some(&4096));
    assert!(is_offered_video_cache(256));
    assert!(!is_offered_video_cache(300));
}

#[test]
fn read_ahead_offers_the_four_shares_the_player_supports() {
    assert_eq!(READ_AHEAD_CHOICES_PERCENT, [25, 50, 75, 90]);
    assert!(is_offered_read_ahead(75));
    assert!(!is_offered_read_ahead(100));
}

#[test]
fn storage_limit_offers_1_to_10_gb() {
    assert_eq!(STORAGE_LIMIT_CHOICES_GB.len(), 10);
    assert!(is_offered_storage_gb(1));
    assert!(is_offered_storage_gb(10));
    assert!(!is_offered_storage_gb(0));
    assert!(!is_offered_storage_gb(11));
}
