use super::BudgetOverrides;
use crate::{HelperIdle, HelperWindows, ResidentPlugins, VideoRenderer};

#[test]
fn nothing_changed_by_default() {
    let overrides = BudgetOverrides::default();
    assert!(overrides.is_empty());
    assert_eq!(overrides.changed_count(), 0);
}

#[test]
fn each_changed_field_counts_once() {
    let overrides = BudgetOverrides {
        resident_plugins: Some(ResidentPlugins::Limit(3)),
        video_cache_mib: Some(512),
        ..BudgetOverrides::default()
    };
    assert!(!overrides.is_empty());
    assert_eq!(overrides.changed_count(), 2);
}

#[test]
fn the_helper_idle_choice_counts_as_a_change() {
    let overrides = BudgetOverrides {
        helper_idle: Some(HelperIdle::After(15)),
        ..BudgetOverrides::default()
    };
    assert_eq!(overrides.changed_count(), 1);
}

#[test]
fn the_video_renderer_choice_counts_as_a_change() {
    let overrides = BudgetOverrides {
        video_renderer: Some(VideoRenderer::Gpu),
        ..BudgetOverrides::default()
    };
    assert_eq!(overrides.changed_count(), 1);
}

#[test]
fn the_helper_windows_choice_counts_as_a_change() {
    let overrides = BudgetOverrides {
        helper_windows: Some(HelperWindows::new(8)),
        ..BudgetOverrides::default()
    };
    assert_eq!(overrides.changed_count(), 1);
}
