//! The profile the viewer picked in Settings.

use crate::MemoryTier;

/// Which preset applies: the detected one (`Auto`) or one the viewer named.
/// "Custom" is not a choice: it is any profile plus at least one override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfileChoice {
    /// Follow the PC's detected tier.
    #[default]
    Auto,
    /// Always the low-end preset.
    LowEnd,
    /// Always the balanced preset.
    Balanced,
    /// Always the high-end preset.
    HighEnd,
}

impl ProfileChoice {
    /// Every choice, in the order Settings lists them.
    pub const ALL: [ProfileChoice; 4] = [Self::Auto, Self::LowEnd, Self::Balanced, Self::HighEnd];

    /// The tier this choice uses on a PC whose detected tier is `detected`.
    pub fn tier(self, detected: MemoryTier) -> MemoryTier {
        match self {
            Self::Auto => detected,
            Self::LowEnd => MemoryTier::LowEnd,
            Self::Balanced => MemoryTier::Balanced,
            Self::HighEnd => MemoryTier::HighEnd,
        }
    }

    /// The text kept in the settings database.
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::LowEnd => "low",
            Self::Balanced => "balanced",
            Self::HighEnd => "high",
        }
    }

    /// Reads [`Self::as_text`] back; anything else is not accepted.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.as_text() == text)
    }
}

#[cfg(test)]
#[path = "profile_choice_tests.rs"]
mod tests;
