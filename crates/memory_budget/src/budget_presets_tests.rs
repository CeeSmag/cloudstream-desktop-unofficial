use super::preset;
use crate::budget_choices::{is_offered_read_ahead, is_offered_storage_gb, is_offered_video_cache};
use crate::{MemoryTier, ResidentPlugins};

const TIERS: [MemoryTier; 3] = [
    MemoryTier::LowEnd,
    MemoryTier::Balanced,
    MemoryTier::HighEnd,
];

#[test]
fn every_preset_value_is_one_the_settings_screen_offers() {
    for tier in TIERS {
        let budget = preset(tier);
        assert!(
            ResidentPlugins::CHOICES.contains(&budget.resident_plugins),
            "{tier:?}"
        );
        assert!(is_offered_video_cache(budget.video_cache_mib), "{tier:?}");
        assert!(
            is_offered_read_ahead(budget.video_read_ahead_percent),
            "{tier:?}"
        );
        assert!(is_offered_storage_gb(budget.storage_limit_gb), "{tier:?}");
    }
}

#[test]
fn bigger_tiers_are_never_more_restricted_than_smaller_ones() {
    let (low, mid, high) = (
        preset(MemoryTier::LowEnd),
        preset(MemoryTier::Balanced),
        preset(MemoryTier::HighEnd),
    );
    let rank = |plugins: ResidentPlugins| plugins.as_limit().unwrap_or(usize::MAX);
    assert!(rank(low.resident_plugins) <= rank(mid.resident_plugins));
    assert!(rank(mid.resident_plugins) <= rank(high.resident_plugins));
    assert!(low.video_cache_mib <= mid.video_cache_mib);
    assert!(mid.video_cache_mib <= high.video_cache_mib);
}

#[test]
fn the_low_end_preset_keeps_one_plugin_process_and_a_small_video_cache() {
    let low = preset(MemoryTier::LowEnd);
    assert_eq!(low.resident_plugins, ResidentPlugins::Limit(1));
    assert_eq!(low.video_cache_mib, 128);
}
