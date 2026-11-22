//! A rough upper estimate of what a budget can use, for the Settings warning. Never a limit.

use crate::MemoryBudget;

/// The app process at idle Home (about 356 MiB measured after the poster cap; `docs/MEMORY.md`).
pub(crate) const APP_BASELINE_MIB: u64 = 360;
/// One plugin process (the JVM) (about 221-230 MiB measured; `docs/MEMORY.md`).
pub(crate) const PLUGIN_PROCESS_MIB: u64 = 230;
/// The warning shows when the estimate passes this share (percent) of the PC's RAM.
const COMFORT_SHARE_PERCENT: u64 = 50;

/// The most the budget could use, in MiB, with `installed_plugins` plugins installed.
pub fn estimate_worst_case_mib(budget: &MemoryBudget, installed_plugins: usize) -> u64 {
    let installed = u64::try_from(installed_plugins).unwrap_or(u64::MAX);
    let resident = budget
        .resident_plugins
        .as_limit()
        .map_or(installed, |limit| {
            u64::try_from(limit).unwrap_or(u64::MAX).min(installed)
        });
    APP_BASELINE_MIB
        .saturating_add(resident.saturating_mul(PLUGIN_PROCESS_MIB))
        .saturating_add(u64::from(budget.video_cache_mib))
}

/// Whether `estimate_mib` is more than half of `total_mib`. Unknown RAM (0) never warns.
pub fn is_heavy(estimate_mib: u64, total_mib: u64) -> bool {
    total_mib > 0
        && estimate_mib.saturating_mul(100) > total_mib.saturating_mul(COMFORT_SHARE_PERCENT)
}

#[cfg(test)]
#[path = "worst_case_tests.rs"]
mod tests;
