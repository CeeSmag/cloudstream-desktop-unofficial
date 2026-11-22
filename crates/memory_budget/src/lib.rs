//! Memory limits for plugins, video and storage, resolved from the PC's RAM, a profile and the
//! viewer's own choices. Pure: no I/O, no logging, no clock. See `docs/MEMORY.md`.
//!
//! Resolution is layered: `preset of the tier <- the viewer's overrides`, into one immutable
//! [`MemoryBudget`] that the app hands to each component by constructor. Poster and picture limits
//! are deliberately not here: they are fixed for every PC.
#![forbid(unsafe_code)]

mod budget_choices;
mod budget_overrides;
mod budget_presets;
mod helper_idle;
mod memory_budget;
mod memory_tier;
mod profile_choice;
mod resident_plugins;
mod system_probe;
mod worst_case;

pub use budget_choices::{
    is_offered_read_ahead, is_offered_storage_gb, is_offered_video_cache,
    READ_AHEAD_CHOICES_PERCENT, STORAGE_LIMIT_CHOICES_GB, VIDEO_CACHE_CHOICES_MIB,
};
pub use budget_overrides::BudgetOverrides;
pub use helper_idle::HelperIdle;
pub use memory_budget::MemoryBudget;
pub use memory_tier::MemoryTier;
pub use profile_choice::ProfileChoice;
pub use resident_plugins::ResidentPlugins;
pub use system_probe::{FixedSystem, SystemFacts, SystemProbe};
pub use worst_case::{estimate_worst_case_mib, is_heavy};
