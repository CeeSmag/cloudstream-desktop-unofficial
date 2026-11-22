//! The profile and the viewer's own choices behind Settings > General > "Performance & memory".
//!
//! The limits themselves keep their old keys (`video_cache_mb`, `storage_cap_gb`,
//! `plugin_worker_limit`, ...): a stored value is the viewer's override, a missing one follows the
//! profile. Resolution is `preset of the tier <- overrides` (see the `memory_budget` crate).

use memory_budget::{
    estimate_worst_case_mib, is_heavy, BudgetOverrides, MemoryBudget, MemoryTier, ProfileChoice,
    SystemFacts,
};

use crate::cloudflare_setting::HELPER_IDLE_KEY;
use crate::plugin_worker_setting::KEY as WORKER_KEY;
use crate::storage_cap_setting::STORAGE_CAP_KEY;
use crate::video_cache_setting::{AHEAD_KEY, SIZE_KEY};
use crate::video_renderer_setting::VIDEO_RENDERER_KEY;
use crate::{ServiceError, SettingsService};

const PROFILE_KEY: &str = "perf_profile";
const MIGRATED_KEY: &str = "perf_migrated";
/// Bumped when a later release needs another one-time clean-up of old values.
const MIGRATION_VERSION: &str = "1";

/// What Settings shows about the PC and the limits in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerformanceSummary {
    /// The profile the viewer picked.
    pub profile: ProfileChoice,
    /// The tier this PC's RAM falls in.
    pub detected: MemoryTier,
    /// The tier whose preset is in force (`detected` when the profile is Auto).
    pub effective: MemoryTier,
    /// The PC.
    pub system: SystemFacts,
    /// How many limits the viewer changed by hand ("Custom: N options changed").
    pub changed: usize,
    /// A rough upper estimate of what the limits can use, in MiB.
    pub worst_case_mib: u64,
    /// Whether that estimate is more than half of the PC's RAM.
    pub heavy: bool,
}

impl SettingsService {
    /// The profile the viewer picked; Auto until they pick one (or when the stored text is unknown).
    pub fn profile_choice(&self) -> Result<ProfileChoice, ServiceError> {
        Ok(self
            .stored(PROFILE_KEY)?
            .as_deref()
            .and_then(ProfileChoice::parse)
            .unwrap_or_default())
    }

    /// Remembers the profile.
    pub fn set_profile_choice(&self, choice: ProfileChoice) -> Result<(), ServiceError> {
        self.store_value(PROFILE_KEY, choice.as_text())
    }

    /// What the viewer changed by hand: one field per stored limit that is readable.
    pub fn performance_overrides(&self) -> Result<BudgetOverrides, ServiceError> {
        Ok(BudgetOverrides {
            resident_plugins: self.stored_worker_choice()?,
            video_cache_mib: self.stored_number(SIZE_KEY)?,
            video_read_ahead_percent: self.stored_number(AHEAD_KEY)?,
            storage_limit_gb: self.stored_number(STORAGE_CAP_KEY)?,
            helper_idle: self.stored_helper_idle()?,
            video_renderer: self.stored_video_renderer()?,
        })
    }

    /// The limits in force for this PC, profile and set of changes.
    pub fn performance_budget(&self) -> Result<MemoryBudget, ServiceError> {
        let tier = self.profile_choice()?.tier(self.detected_tier());
        Ok(MemoryBudget::resolve(tier, &self.performance_overrides()?))
    }

    /// The tier this PC's RAM falls in.
    pub fn detected_tier(&self) -> MemoryTier {
        MemoryTier::from_total_mib(self.system().total_mib)
    }

    /// "Reset to recommended": back to Auto, every change forgotten.
    pub fn reset_performance(&self) -> Result<(), ServiceError> {
        for key in [
            PROFILE_KEY,
            WORKER_KEY,
            SIZE_KEY,
            AHEAD_KEY,
            STORAGE_CAP_KEY,
            HELPER_IDLE_KEY,
            VIDEO_RENDERER_KEY,
        ] {
            self.remove_value(key)?;
        }
        Ok(())
    }

    /// What Settings shows, with `installed_plugins` plugins installed.
    pub fn performance_summary(
        &self,
        installed_plugins: usize,
    ) -> Result<PerformanceSummary, ServiceError> {
        let profile = self.profile_choice()?;
        let detected = self.detected_tier();
        let budget = self.performance_budget()?;
        let worst_case_mib = estimate_worst_case_mib(&budget, installed_plugins);
        Ok(PerformanceSummary {
            profile,
            detected,
            effective: profile.tier(detected),
            system: self.system(),
            changed: self.performance_overrides()?.changed_count(),
            worst_case_mib,
            heavy: is_heavy(worst_case_mib, self.system().total_mib),
        })
    }

    /// One-time clean-up when the profiles arrive: the plugin limits the old app wrote itself
    /// ("0" for installs that already had data, "1" for new ones) are not choices, so they are
    /// dropped and the profile decides. A "2" or more could only have been chosen by the viewer
    /// and is kept; so are the video cache and storage choices.
    pub fn migrate_performance_settings(&self) -> Result<(), ServiceError> {
        if self.stored(MIGRATED_KEY)?.is_some() {
            return Ok(());
        }
        if let Some(text) = self.stored(WORKER_KEY)? {
            if matches!(text.trim(), "0" | "1") {
                tracing::info!(stored = %text.trim(), "dropping a plugin process limit the app wrote itself");
                self.remove_value(WORKER_KEY)?;
            }
        }
        self.store_value(MIGRATED_KEY, MIGRATION_VERSION)
    }

    fn stored_number<T: std::str::FromStr>(&self, key: &str) -> Result<Option<T>, ServiceError> {
        Ok(self.stored(key)?.and_then(|text| text.trim().parse().ok()))
    }
}

#[cfg(test)]
#[path = "performance_settings_tests.rs"]
mod tests;
