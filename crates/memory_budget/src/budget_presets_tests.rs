use super::preset;
use crate::budget_choices::{is_offered_read_ahead, is_offered_storage_gb, is_offered_video_cache};
use crate::{HelperIdle, HelperWindows, MemoryTier, ResidentPlugins, VideoRenderer};

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
        assert!(
            HelperIdle::CHOICES.contains(&budget.helper_idle),
            "{tier:?}"
        );
        assert!(
            HelperWindows::CHOICES.contains(&budget.helper_windows),
            "{tier:?}"
        );
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

#[test]
fn a_bigger_pc_keeps_the_helper_open_longer() {
    let (low, mid, high) = (
        preset(MemoryTier::LowEnd).helper_idle,
        preset(MemoryTier::Balanced).helper_idle,
        preset(MemoryTier::HighEnd).helper_idle,
    );
    assert_eq!(
        (low, mid, high),
        (
            HelperIdle::After(2),
            HelperIdle::After(5),
            HelperIdle::Never
        )
    );
}

#[test]
fn every_tier_starts_on_the_software_video_path_until_the_gpu_path_is_verified_more_widely() {
    for tier in TIERS {
        assert_eq!(preset(tier).video_renderer, VideoRenderer::Cpu, "{tier:?}");
    }
}

#[test]
fn a_bigger_pc_keeps_more_verified_browser_windows() {
    let counts = [
        MemoryTier::LowEnd,
        MemoryTier::Balanced,
        MemoryTier::HighEnd,
    ]
    .map(|tier| preset(tier).helper_windows.count());
    assert_eq!(counts, [2, 4, 8]);
}
