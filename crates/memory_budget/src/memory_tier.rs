//! The size class of the PC, from its RAM. Picks the preset that "Auto" uses.

use crate::SystemProbe;

/// Up to this much RAM (MiB) a PC counts as low-end: a "4 GB" PC reports about 3.9 GB and a
/// "6 GB" one about 5.8 GB. Provisional: to be tuned from measurements (`docs/MEMORY.md`).
pub(crate) const LOW_END_MAX_MIB: u64 = 6 * 1024;
/// Up to this much RAM (MiB) a PC counts as balanced: "8 GB" and "12 GB" PCs. Above it: high-end.
pub(crate) const BALANCED_MAX_MIB: u64 = 12 * 1024;

/// How much room the PC leaves the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryTier {
    /// Tight limits.
    LowEnd,
    /// Moderate limits.
    Balanced,
    /// Generous limits.
    HighEnd,
}

impl MemoryTier {
    /// The tier for `total_mib` of RAM. Unknown RAM (0) counts as low-end, the safe side.
    pub fn from_total_mib(total_mib: u64) -> Self {
        if total_mib <= LOW_END_MAX_MIB {
            Self::LowEnd
        } else if total_mib <= BALANCED_MAX_MIB {
            Self::Balanced
        } else {
            Self::HighEnd
        }
    }

    /// The tier of the PC the probe describes.
    pub fn detect(probe: &dyn SystemProbe) -> Self {
        Self::from_total_mib(probe.facts().total_mib)
    }

    /// The name shown in Settings.
    pub fn label(self) -> &'static str {
        match self {
            Self::LowEnd => "Low-end",
            Self::Balanced => "Balanced",
            Self::HighEnd => "High-end",
        }
    }
}

#[cfg(test)]
#[path = "memory_tier_tests.rs"]
mod tests;
