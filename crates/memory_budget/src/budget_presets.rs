//! The preset of each tier. Data, not logic: a new tier or field is a new row or value here.

use crate::{HelperIdle, MemoryBudget, MemoryTier, ResidentPlugins};

/// The limits a PC of `tier` starts with. Values come from the S1 measurement in
/// `docs/MEMORY.md`: a plugin process costs about 230 MiB, the video cache is read as written.
pub(crate) fn preset(tier: MemoryTier) -> MemoryBudget {
    match tier {
        MemoryTier::LowEnd => MemoryBudget {
            resident_plugins: ResidentPlugins::Limit(1),
            video_cache_mib: 128,
            video_read_ahead_percent: 50,
            storage_limit_gb: 2,
            helper_idle: HelperIdle::After(2),
        },
        MemoryTier::Balanced => MemoryBudget {
            resident_plugins: ResidentPlugins::Limit(2),
            video_cache_mib: 256,
            video_read_ahead_percent: 75,
            storage_limit_gb: 2,
            helper_idle: HelperIdle::After(5),
        },
        MemoryTier::HighEnd => MemoryBudget {
            resident_plugins: ResidentPlugins::Limit(4),
            video_cache_mib: 1024,
            video_read_ahead_percent: 75,
            storage_limit_gb: 2,
            helper_idle: HelperIdle::Never,
        },
    }
}

#[cfg(test)]
#[path = "budget_presets_tests.rs"]
mod tests;
