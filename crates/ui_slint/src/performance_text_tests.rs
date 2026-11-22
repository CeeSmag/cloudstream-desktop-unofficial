use app_services::PerformanceSummary;
use memory_budget::{MemoryTier, ProfileChoice, SystemFacts};

use super::{
    describe, helper_idle_from_label, helper_idle_label, helper_idle_labels,
    helper_windows_from_label, helper_windows_label, helper_windows_labels, profile_from_label,
    profile_label, profile_labels, video_renderer_from_label, video_renderer_label,
    video_renderer_labels,
};

fn summary(
    profile: ProfileChoice,
    total_mib: u64,
    changed: usize,
    worst: u64,
) -> PerformanceSummary {
    let detected = MemoryTier::from_total_mib(total_mib);
    PerformanceSummary {
        profile,
        detected,
        effective: profile.tier(detected),
        system: SystemFacts {
            total_mib,
            cpu_cores: 4,
        },
        changed,
        worst_case_mib: worst,
        heavy: total_mib > 0 && worst * 2 > total_mib,
    }
}

#[test]
fn the_four_profiles_are_listed_with_auto_first_and_recommended() {
    assert_eq!(
        profile_labels(),
        ["Auto (recommended)", "Low-end", "Balanced", "High-end"]
    );
}

#[test]
fn every_label_maps_back_to_its_profile() {
    for choice in ProfileChoice::ALL {
        assert_eq!(profile_from_label(profile_label(choice)), Some(choice));
    }
    assert_eq!(profile_from_label("Turbo"), None);
}

#[test]
fn the_pc_line_names_ram_cores_and_the_suggested_profile() {
    let view = describe(&summary(ProfileChoice::Auto, 7_800, 0, 800));
    assert_eq!(
        view.pc_text,
        "This PC: 8 GB RAM, 4 cores · suggested profile: Balanced"
    );
}

#[test]
fn unknown_ram_is_said_plainly() {
    let view = describe(&summary(ProfileChoice::Auto, 0, 0, 800));
    assert!(view.pc_text.contains("memory unknown"), "{}", view.pc_text);
}

#[test]
fn the_state_line_says_which_preset_is_followed_or_how_many_options_changed() {
    let auto = describe(&summary(ProfileChoice::Auto, 3_900, 0, 800));
    assert_eq!(auto.state_text, "Following the Low-end preset for this PC");
    let named = describe(&summary(ProfileChoice::HighEnd, 3_900, 0, 800));
    assert_eq!(named.state_text, "Using the High-end preset");
    let one = describe(&summary(ProfileChoice::Auto, 7_800, 1, 800));
    assert_eq!(
        one.state_text,
        "Custom: 1 option changed from the Balanced preset"
    );
    let two = describe(&summary(ProfileChoice::Auto, 7_800, 2, 800));
    assert_eq!(
        two.state_text,
        "Custom: 2 options changed from the Balanced preset"
    );
}

#[test]
fn reset_is_offered_only_when_something_differs_from_auto() {
    assert!(!describe(&summary(ProfileChoice::Auto, 7_800, 0, 800)).can_reset);
    assert!(describe(&summary(ProfileChoice::Auto, 7_800, 1, 800)).can_reset);
    assert!(describe(&summary(ProfileChoice::LowEnd, 7_800, 0, 800)).can_reset);
}

#[test]
fn the_warning_appears_only_for_a_heavy_budget_and_names_both_sizes() {
    let calm = describe(&summary(ProfileChoice::Auto, 31_000, 0, 2_300));
    assert_eq!(calm.warning, "");
    let heavy = describe(&summary(ProfileChoice::HighEnd, 3_900, 0, 2_400));
    assert_eq!(
        heavy.warning,
        "These settings can use up to about 2.3 GB of this PC's 4 GB. It may slow down."
    );
}

#[test]
fn the_helper_idle_choices_read_as_minutes_and_never() {
    assert_eq!(
        helper_idle_labels(),
        ["1 min", "2 min", "5 min", "15 min", "Never"]
    );
}

#[test]
fn every_helper_idle_label_maps_back() {
    for choice in memory_budget::HelperIdle::CHOICES {
        assert_eq!(
            helper_idle_from_label(&helper_idle_label(choice)),
            Some(choice)
        );
    }
    assert_eq!(helper_idle_from_label("Soon"), None);
}

#[test]
fn the_video_output_choices_say_what_they_are() {
    assert_eq!(
        video_renderer_labels(),
        ["Software (works everywhere)", "GPU (no picture copies)"]
    );
}

#[test]
fn every_video_output_label_maps_back() {
    for choice in memory_budget::VideoRenderer::CHOICES {
        assert_eq!(
            video_renderer_from_label(video_renderer_label(choice)),
            Some(choice)
        );
    }
    assert_eq!(video_renderer_from_label("Turbo"), None);
}

#[test]
fn the_helper_window_choices_read_as_counts_of_windows() {
    assert_eq!(
        helper_windows_labels(),
        ["1 window", "2 windows", "4 windows", "8 windows"]
    );
}

#[test]
fn every_helper_window_label_maps_back() {
    for choice in memory_budget::HelperWindows::CHOICES {
        assert_eq!(
            helper_windows_from_label(&helper_windows_label(choice)),
            Some(choice)
        );
    }
    assert_eq!(helper_windows_from_label("3 windows"), None);
}
