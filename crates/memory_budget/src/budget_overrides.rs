//! What the viewer changed by hand. Only changed fields are set, so a later profile change still
//! reaches every field they left alone.

use crate::{HelperIdle, HelperWindows, ResidentPlugins, VideoRenderer};

/// The viewer's own choices; `None` means "follow the profile".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BudgetOverrides {
    /// Plugin processes kept running.
    pub resident_plugins: Option<ResidentPlugins>,
    /// Video cache size, MiB.
    pub video_cache_mib: Option<u32>,
    /// Share of the video cache read ahead, percent.
    pub video_read_ahead_percent: Option<u8>,
    /// Data folder limit, GB.
    pub storage_limit_gb: Option<u32>,
    /// Idle time before the Cloudflare helper is closed.
    pub helper_idle: Option<HelperIdle>,
    /// Verified browser windows the helper keeps.
    pub helper_windows: Option<HelperWindows>,
    /// Where the video player draws its pictures.
    pub video_renderer: Option<VideoRenderer>,
}

impl BudgetOverrides {
    /// How many fields the viewer changed ("Custom: 2 options changed").
    pub fn changed_count(&self) -> usize {
        [
            self.resident_plugins.is_some(),
            self.video_cache_mib.is_some(),
            self.video_read_ahead_percent.is_some(),
            self.storage_limit_gb.is_some(),
            self.helper_idle.is_some(),
            self.helper_windows.is_some(),
            self.video_renderer.is_some(),
        ]
        .into_iter()
        .filter(|changed| *changed)
        .count()
    }

    /// Whether the viewer changed nothing (the profile applies as it is).
    pub fn is_empty(&self) -> bool {
        self.changed_count() == 0
    }
}

#[cfg(test)]
#[path = "budget_overrides_tests.rs"]
mod tests;
