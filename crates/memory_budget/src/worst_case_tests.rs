use super::{
    estimate_worst_case_mib, is_heavy, APP_BASELINE_MIB, HELPER_WINDOW_MIB, PLUGIN_PROCESS_MIB,
};
use crate::{HelperWindows, MemoryBudget, MemoryTier, ResidentPlugins};

fn budget(plugins: ResidentPlugins, video_cache_mib: u32) -> MemoryBudget {
    MemoryBudget {
        resident_plugins: plugins,
        video_cache_mib,
        helper_windows: HelperWindows::new(1),
        ..MemoryBudget::resolve(MemoryTier::Balanced, &Default::default())
    }
}

#[test]
fn estimate_adds_the_app_the_resident_plugins_and_the_video_cache() {
    let estimate = estimate_worst_case_mib(&budget(ResidentPlugins::Limit(2), 256), 5);
    assert_eq!(estimate, APP_BASELINE_MIB + 2 * PLUGIN_PROCESS_MIB + 256 + HELPER_WINDOW_MIB);
}

#[test]
fn only_installed_plugins_can_be_resident() {
    let estimate = estimate_worst_case_mib(&budget(ResidentPlugins::Limit(4), 128), 1);
    assert_eq!(estimate, APP_BASELINE_MIB + PLUGIN_PROCESS_MIB + 128 + HELPER_WINDOW_MIB);
}

#[test]
fn unlimited_means_every_installed_plugin() {
    let estimate = estimate_worst_case_mib(&budget(ResidentPlugins::Unlimited, 128), 6);
    assert_eq!(estimate, APP_BASELINE_MIB + 6 * PLUGIN_PROCESS_MIB + 128 + HELPER_WINDOW_MIB);
}

#[test]
fn heavy_means_more_than_half_of_the_pcs_ram() {
    assert!(is_heavy(2_100, 4_000));
    assert!(!is_heavy(2_000, 4_000));
    assert!(!is_heavy(10_000, 0), "unknown RAM never warns");
}

#[test]
fn each_kept_helper_window_adds_a_browser_process() {
    let one = estimate_worst_case_mib(&budget(ResidentPlugins::Limit(1), 128), 1);
    let four = estimate_worst_case_mib(
        &MemoryBudget {
            helper_windows: HelperWindows::new(4),
            ..budget(ResidentPlugins::Limit(1), 128)
        },
        1,
    );
    assert_eq!(four - one, 3 * HELPER_WINDOW_MIB);
}
