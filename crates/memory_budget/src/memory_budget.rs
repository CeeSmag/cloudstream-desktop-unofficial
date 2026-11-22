//! The resolved limits every component receives.

use crate::budget_choices::{is_offered_read_ahead, is_offered_storage_gb, is_offered_video_cache};
use crate::budget_presets::preset;
use crate::{
    BudgetOverrides, HelperIdle, HelperWindows, MemoryTier, ResidentPlugins, VideoRenderer,
};

/// The limits in force: immutable, built once from a tier and the viewer's overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBudget {
    /// Plugin processes kept running.
    pub resident_plugins: ResidentPlugins,
    /// Video cache size, MiB.
    pub video_cache_mib: u32,
    /// Share of the video cache read ahead, percent.
    pub video_read_ahead_percent: u8,
    /// Data folder limit, GB.
    pub storage_limit_gb: u32,
    /// How long the Cloudflare verification helper may sit idle before it is closed.
    pub helper_idle: HelperIdle,
    /// How many verified browser windows the helper keeps.
    pub helper_windows: HelperWindows,
    /// Where the video player draws its pictures.
    pub video_renderer: VideoRenderer,
}

impl MemoryBudget {
    /// `preset of the tier <- the viewer's overrides`. An override that is not an offered value is
    /// ignored, so a hand-edited database can never produce an unsupported limit.
    pub fn resolve(tier: MemoryTier, overrides: &BudgetOverrides) -> Self {
        let base = preset(tier);
        Self {
            resident_plugins: overrides
                .resident_plugins
                .filter(|choice| ResidentPlugins::CHOICES.contains(choice))
                .unwrap_or(base.resident_plugins),
            video_cache_mib: overrides
                .video_cache_mib
                .filter(|mib| is_offered_video_cache(*mib))
                .unwrap_or(base.video_cache_mib),
            video_read_ahead_percent: overrides
                .video_read_ahead_percent
                .filter(|percent| is_offered_read_ahead(*percent))
                .unwrap_or(base.video_read_ahead_percent),
            storage_limit_gb: overrides
                .storage_limit_gb
                .filter(|gb| is_offered_storage_gb(*gb))
                .unwrap_or(base.storage_limit_gb),
            helper_idle: overrides
                .helper_idle
                .filter(|choice| HelperIdle::CHOICES.contains(choice))
                .unwrap_or(base.helper_idle),
            helper_windows: overrides
                .helper_windows
                .filter(|choice| HelperWindows::CHOICES.contains(choice))
                .unwrap_or(base.helper_windows),
            video_renderer: overrides
                .video_renderer
                .filter(|choice| VideoRenderer::CHOICES.contains(choice))
                .unwrap_or(base.video_renderer),
        }
    }
}

#[cfg(test)]
#[path = "memory_budget_tests.rs"]
mod tests;
