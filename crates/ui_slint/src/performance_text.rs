//! The words of Settings > General > "Performance & memory": profile names, what the PC is, which
//! preset is followed and the warning. Pure, so it is tested without a window.

use app_services::PerformanceSummary;
use memory_budget::ProfileChoice;

/// MiB in a GB, for showing RAM sizes.
const MIB_PER_GB: u64 = 1024;

/// What the Performance card shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PerformanceView {
    /// The profile drop-down's current value.
    pub(crate) profile_value: String,
    /// "This PC: 8 GB RAM, 4 cores · suggested profile: Balanced".
    pub(crate) pc_text: String,
    /// Which preset is followed, or how many options differ from it.
    pub(crate) state_text: String,
    /// Empty unless the limits can use more than half of the RAM.
    pub(crate) warning: String,
    /// Whether "Reset to recommended" has anything to undo.
    pub(crate) can_reset: bool,
}

/// The drop-down's name of a profile.
pub(crate) fn profile_label(choice: ProfileChoice) -> &'static str {
    match choice {
        ProfileChoice::Auto => "Auto (recommended)",
        ProfileChoice::LowEnd => "Low-end",
        ProfileChoice::Balanced => "Balanced",
        ProfileChoice::HighEnd => "High-end",
    }
}

/// Every profile's drop-down name, in the order they are listed.
pub(crate) fn profile_labels() -> Vec<String> {
    ProfileChoice::ALL
        .iter()
        .map(|choice| profile_label(*choice).to_owned())
        .collect()
}

/// The profile a drop-down name stands for.
pub(crate) fn profile_from_label(label: &str) -> Option<ProfileChoice> {
    ProfileChoice::ALL
        .into_iter()
        .find(|choice| profile_label(*choice) == label)
}

/// The card's texts for `summary`.
pub(crate) fn describe(summary: &PerformanceSummary) -> PerformanceView {
    PerformanceView {
        profile_value: profile_label(summary.profile).to_owned(),
        pc_text: pc_text(summary),
        state_text: state_text(summary),
        warning: warning(summary),
        can_reset: summary.changed > 0 || summary.profile != ProfileChoice::Auto,
    }
}

fn pc_text(summary: &PerformanceSummary) -> String {
    let system = summary.system;
    if system.total_mib == 0 {
        return "This PC: memory unknown (treated as low-end)".to_owned();
    }
    let cores = system.cpu_cores;
    let unit = if cores == 1 { "core" } else { "cores" };
    format!(
        "This PC: {} GB RAM, {cores} {unit} · suggested profile: {}",
        whole_gb(system.total_mib),
        summary.detected.label()
    )
}

fn state_text(summary: &PerformanceSummary) -> String {
    let preset = summary.effective.label();
    match (summary.changed, summary.profile) {
        (0, ProfileChoice::Auto) => format!("Following the {preset} preset for this PC"),
        (0, _) => format!("Using the {preset} preset"),
        (1, _) => format!("Custom: 1 option changed from the {preset} preset"),
        (count, _) => format!("Custom: {count} options changed from the {preset} preset"),
    }
}

fn warning(summary: &PerformanceSummary) -> String {
    if !summary.heavy {
        return String::new();
    }
    let used_gb = summary.worst_case_mib as f64 / MIB_PER_GB as f64;
    format!(
        "These settings can use up to about {used_gb:.1} GB of this PC's {} GB. It may slow down.",
        whole_gb(summary.system.total_mib)
    )
}

/// RAM in whole GB, rounded to nearest (a "4 GB" PC reports about 3.9 GB).
fn whole_gb(total_mib: u64) -> u64 {
    (total_mib + MIB_PER_GB / 2) / MIB_PER_GB
}

#[cfg(test)]
#[path = "performance_text_tests.rs"]
mod tests;
