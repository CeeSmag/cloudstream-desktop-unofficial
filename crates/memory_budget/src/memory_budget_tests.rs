use super::MemoryBudget;
use crate::budget_presets::preset;
use crate::{
    BudgetOverrides, HelperIdle, HelperWindows, MemoryTier, ResidentPlugins, VideoRenderer,
};

#[test]
fn without_overrides_the_budget_is_the_preset() {
    let budget = MemoryBudget::resolve(MemoryTier::Balanced, &BudgetOverrides::default());
    assert_eq!(budget, preset(MemoryTier::Balanced));
}

#[test]
fn an_offered_override_wins_over_the_preset() {
    let overrides = BudgetOverrides {
        resident_plugins: Some(ResidentPlugins::Unlimited),
        video_cache_mib: Some(2048),
        ..BudgetOverrides::default()
    };
    let budget = MemoryBudget::resolve(MemoryTier::LowEnd, &overrides);
    assert_eq!(budget.resident_plugins, ResidentPlugins::Unlimited);
    assert_eq!(budget.video_cache_mib, 2048);
    assert_eq!(
        budget.video_read_ahead_percent,
        preset(MemoryTier::LowEnd).video_read_ahead_percent,
        "fields the viewer did not change still follow the preset"
    );
}

#[test]
fn a_value_that_is_not_offered_falls_back_to_the_preset() {
    let overrides = BudgetOverrides {
        video_cache_mib: Some(300),
        video_read_ahead_percent: Some(100),
        storage_limit_gb: Some(99),
        ..BudgetOverrides::default()
    };
    let budget = MemoryBudget::resolve(MemoryTier::Balanced, &overrides);
    assert_eq!(budget, preset(MemoryTier::Balanced));
}

#[test]
fn removing_an_override_restores_the_preset_of_the_new_tier() {
    let overrides = BudgetOverrides {
        video_cache_mib: Some(512),
        ..BudgetOverrides::default()
    };
    let with = MemoryBudget::resolve(MemoryTier::LowEnd, &overrides);
    assert_eq!(with.video_cache_mib, 512);
    let without = MemoryBudget::resolve(MemoryTier::HighEnd, &BudgetOverrides::default());
    assert_eq!(
        without.video_cache_mib,
        preset(MemoryTier::HighEnd).video_cache_mib
    );
}

#[test]
fn the_helper_idle_time_follows_the_profile_unless_the_viewer_chose_one() {
    let low = MemoryBudget::resolve(MemoryTier::LowEnd, &BudgetOverrides::default());
    assert_eq!(low.helper_idle, HelperIdle::After(2));
    let chosen = BudgetOverrides {
        helper_idle: Some(HelperIdle::Never),
        ..BudgetOverrides::default()
    };
    assert_eq!(
        MemoryBudget::resolve(MemoryTier::LowEnd, &chosen).helper_idle,
        HelperIdle::Never
    );
    let not_offered = BudgetOverrides {
        helper_idle: Some(HelperIdle::After(7)),
        ..BudgetOverrides::default()
    };
    assert_eq!(
        MemoryBudget::resolve(MemoryTier::LowEnd, &not_offered).helper_idle,
        HelperIdle::After(2),
        "a value that is not offered falls back to the preset"
    );
}

#[test]
fn the_video_renderer_is_software_until_the_viewer_chooses_gpu() {
    let none = MemoryBudget::resolve(MemoryTier::HighEnd, &BudgetOverrides::default());
    assert_eq!(none.video_renderer, VideoRenderer::Cpu);
    let chosen = BudgetOverrides {
        video_renderer: Some(VideoRenderer::Gpu),
        ..BudgetOverrides::default()
    };
    assert_eq!(
        MemoryBudget::resolve(MemoryTier::LowEnd, &chosen).video_renderer,
        VideoRenderer::Gpu
    );
}

#[test]
fn the_kept_helper_windows_follow_the_profile_unless_the_viewer_chose_a_count() {
    let low = MemoryBudget::resolve(MemoryTier::LowEnd, &BudgetOverrides::default());
    assert_eq!(low.helper_windows, HelperWindows::new(2));
    let chosen = BudgetOverrides {
        helper_windows: Some(HelperWindows::new(8)),
        ..BudgetOverrides::default()
    };
    assert_eq!(
        MemoryBudget::resolve(MemoryTier::LowEnd, &chosen).helper_windows,
        HelperWindows::new(8)
    );
    let not_offered = BudgetOverrides {
        helper_windows: Some(HelperWindows::new(3)),
        ..BudgetOverrides::default()
    };
    assert_eq!(
        MemoryBudget::resolve(MemoryTier::LowEnd, &not_offered).helper_windows,
        HelperWindows::new(2),
        "a count that is not offered falls back to the preset"
    );
}
