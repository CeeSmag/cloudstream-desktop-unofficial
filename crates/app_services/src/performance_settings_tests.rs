use std::sync::Arc;

use memory_budget::{MemoryTier, ProfileChoice, ResidentPlugins, SystemFacts};
use storage::{Database, SettingsStore};

use crate::video_cache_setting::VideoCache;
use crate::SettingsService;

fn service_with_ram(total_mib: u64) -> SettingsService {
    let database = Arc::new(Database::open_in_memory().expect("memory db"));
    SettingsService::new(SettingsStore::new(database)).with_system(SystemFacts {
        total_mib,
        cpu_cores: 4,
    })
}

const FOUR_GB: u64 = 3_900;
const EIGHT_GB: u64 = 7_800;
const THIRTY_TWO_GB: u64 = 31_000;

#[test]
fn the_profile_is_auto_until_chosen_and_round_trips() {
    let service = service_with_ram(EIGHT_GB);
    assert_eq!(service.profile_choice().expect("read"), ProfileChoice::Auto);
    service
        .set_profile_choice(ProfileChoice::HighEnd)
        .expect("set");
    assert_eq!(
        service.profile_choice().expect("read"),
        ProfileChoice::HighEnd
    );
}

#[test]
fn an_unknown_stored_profile_counts_as_auto() {
    let service = service_with_ram(EIGHT_GB);
    service.store_value("perf_profile", "turbo").expect("write");
    assert_eq!(service.profile_choice().expect("read"), ProfileChoice::Auto);
}

#[test]
fn auto_gives_each_pc_the_limits_of_its_size() {
    let low = service_with_ram(FOUR_GB);
    assert_eq!(
        low.video_cache().expect("r"),
        VideoCache {
            size_mb: 128,
            ahead_percent: 50
        }
    );
    assert_eq!(low.plugin_worker_limit().expect("r"), Some(1));
    let mid = service_with_ram(EIGHT_GB);
    assert_eq!(
        mid.video_cache().expect("r"),
        VideoCache {
            size_mb: 256,
            ahead_percent: 75
        }
    );
    assert_eq!(mid.plugin_worker_limit().expect("r"), Some(2));
    let high = service_with_ram(THIRTY_TWO_GB);
    assert_eq!(
        high.video_cache().expect("r"),
        VideoCache {
            size_mb: 1024,
            ahead_percent: 75
        }
    );
    assert_eq!(high.plugin_worker_limit().expect("r"), Some(4));
}

#[test]
fn a_named_profile_beats_the_detected_size() {
    let service = service_with_ram(FOUR_GB);
    service
        .set_profile_choice(ProfileChoice::HighEnd)
        .expect("set");
    assert_eq!(service.video_cache().expect("r").size_mb, 1024);
    assert_eq!(service.plugin_worker_limit().expect("r"), Some(4));
}

#[test]
fn what_the_viewer_chose_beats_the_profile_and_counts_as_a_change() {
    let service = service_with_ram(FOUR_GB);
    service
        .set_video_cache(VideoCache {
            size_mb: 512,
            ahead_percent: 90,
        })
        .expect("set");
    service.set_plugin_worker_limit(None).expect("unlimited");
    service.set_storage_cap_gb(5).expect("set");
    assert_eq!(
        service.video_cache().expect("r"),
        VideoCache {
            size_mb: 512,
            ahead_percent: 90
        }
    );
    assert_eq!(service.plugin_worker_limit().expect("r"), None);
    assert_eq!(service.storage_cap_gb().expect("r"), 5);
    let overrides = service.performance_overrides().expect("read");
    assert_eq!(overrides.resident_plugins, Some(ResidentPlugins::Unlimited));
    assert_eq!(overrides.changed_count(), 4);
}

#[test]
fn reset_returns_to_auto_and_forgets_every_override() {
    let service = service_with_ram(FOUR_GB);
    service
        .set_profile_choice(ProfileChoice::HighEnd)
        .expect("set");
    service
        .set_video_cache(VideoCache {
            size_mb: 512,
            ahead_percent: 90,
        })
        .expect("set");
    service.set_storage_cap_gb(5).expect("set");
    service.reset_performance().expect("reset");
    assert_eq!(service.profile_choice().expect("r"), ProfileChoice::Auto);
    assert!(service.performance_overrides().expect("r").is_empty());
    assert_eq!(service.video_cache().expect("r").size_mb, 128);
}

#[test]
fn migration_drops_the_plugin_limits_the_old_app_wrote_itself_once() {
    for written_by_the_old_app in ["0", "1"] {
        let service = service_with_ram(THIRTY_TWO_GB);
        service
            .store_value("plugin_worker_limit", written_by_the_old_app)
            .expect("w");
        service.migrate_performance_settings().expect("migrate");
        assert_eq!(
            service.plugin_worker_limit().expect("r"),
            Some(4),
            "follows Auto now"
        );
    }
}

#[test]
fn migration_keeps_a_limit_only_the_viewer_could_have_chosen() {
    let service = service_with_ram(THIRTY_TWO_GB);
    service.store_value("plugin_worker_limit", "2").expect("w");
    service.migrate_performance_settings().expect("migrate");
    assert_eq!(service.plugin_worker_limit().expect("r"), Some(2));
}

#[test]
fn migration_runs_once_so_a_later_unlimited_choice_stays() {
    let service = service_with_ram(THIRTY_TWO_GB);
    service.migrate_performance_settings().expect("first");
    service
        .set_plugin_worker_limit(None)
        .expect("viewer picks unlimited");
    service.migrate_performance_settings().expect("second");
    assert_eq!(service.plugin_worker_limit().expect("r"), None);
}

#[test]
fn migration_keeps_video_cache_and_storage_choices_as_overrides() {
    let service = service_with_ram(FOUR_GB);
    service
        .set_video_cache(VideoCache {
            size_mb: 2048,
            ahead_percent: 50,
        })
        .expect("set");
    service.migrate_performance_settings().expect("migrate");
    assert_eq!(service.video_cache().expect("r").size_mb, 2048);
}

#[test]
fn the_summary_reports_the_profile_the_size_and_whether_the_budget_is_heavy() {
    let service = service_with_ram(FOUR_GB);
    service
        .set_profile_choice(ProfileChoice::HighEnd)
        .expect("set");
    let summary = service.performance_summary(6).expect("summary");
    assert_eq!(summary.profile, ProfileChoice::HighEnd);
    assert_eq!(summary.detected, MemoryTier::LowEnd);
    assert_eq!(summary.effective, MemoryTier::HighEnd);
    assert_eq!(summary.changed, 0);
    assert!(
        summary.heavy,
        "four plugins and a 1 GB cache are heavy on a 4 GB PC"
    );
    let calm = service_with_ram(THIRTY_TWO_GB)
        .performance_summary(6)
        .expect("summary");
    assert!(!calm.heavy);
}

#[test]
fn the_helper_idle_time_follows_the_profile_a_choice_wins_and_reset_forgets_it() {
    use memory_budget::HelperIdle;
    let low = service_with_ram(FOUR_GB);
    assert_eq!(low.helper_idle().expect("r"), HelperIdle::After(2));
    assert_eq!(
        service_with_ram(THIRTY_TWO_GB).helper_idle().expect("r"),
        HelperIdle::Never
    );
    low.set_helper_idle(HelperIdle::After(15)).expect("set");
    assert_eq!(low.helper_idle().expect("r"), HelperIdle::After(15));
    assert_eq!(low.performance_overrides().expect("r").changed_count(), 1);
    low.reset_performance().expect("reset");
    assert_eq!(low.helper_idle().expect("r"), HelperIdle::After(2));
}
