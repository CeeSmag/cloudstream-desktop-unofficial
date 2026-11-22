use super::BudgetOverrides;
use crate::ResidentPlugins;

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
