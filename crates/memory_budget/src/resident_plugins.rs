//! How many plugin processes may stay running at once.

/// The plugin-process cap. `Unlimited` is its own case, so "0 means unlimited" cannot leak out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidentPlugins {
    /// At most this many plugin processes stay resident.
    Limit(u8),
    /// Every installed plugin stays resident.
    Unlimited,
}

impl ResidentPlugins {
    /// The values the settings screen offers.
    pub const CHOICES: [ResidentPlugins; 6] = [
        Self::Limit(1),
        Self::Limit(2),
        Self::Limit(3),
        Self::Limit(4),
        Self::Limit(6),
        Self::Unlimited,
    ];

    /// The cap in the form the plugin worker pool takes (`None` = unlimited).
    pub fn as_limit(self) -> Option<usize> {
        match self {
            Self::Limit(count) => Some(usize::from(count)),
            Self::Unlimited => None,
        }
    }

    /// From the worker pool's form: `None` and `Some(0)` mean unlimited; huge values saturate.
    pub fn from_limit(limit: Option<usize>) -> Self {
        match limit {
            None | Some(0) => Self::Unlimited,
            Some(count) => Self::Limit(u8::try_from(count).unwrap_or(u8::MAX)),
        }
    }

    /// The text kept in the settings database.
    pub fn to_text(self) -> String {
        match self {
            Self::Limit(count) => count.to_string(),
            Self::Unlimited => "unlimited".to_owned(),
        }
    }

    /// Reads [`Self::to_text`] back; anything else is not accepted.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("unlimited") {
            return Some(Self::Unlimited);
        }
        text.parse::<u8>()
            .ok()
            .filter(|count| *count > 0)
            .map(Self::Limit)
    }
}

#[cfg(test)]
#[path = "resident_plugins_tests.rs"]
mod tests;
