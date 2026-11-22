//! How many verified browser windows the Cloudflare helper keeps.

/// Each kept window is a browser process of its own (about 140-165 MiB measured, `docs/MEMORY.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelperWindows(u8);

impl HelperWindows {
    /// The values the settings screen offers.
    pub const CHOICES: [HelperWindows; 4] = [Self(1), Self(2), Self(4), Self(8)];

    /// A count; not checked against [`Self::CHOICES`] (the budget does that when it resolves).
    pub const fn new(count: u8) -> Self {
        Self(count)
    }

    /// The number of windows.
    pub fn count(self) -> usize {
        usize::from(self.0)
    }

    /// The text kept in the settings database.
    pub fn to_text(self) -> String {
        self.0.to_string()
    }

    /// Reads [`Self::to_text`] back; a count that is not offered is not accepted.
    pub fn parse(text: &str) -> Option<Self> {
        let count = text.trim().parse::<u8>().ok()?;
        Self::CHOICES.contains(&Self(count)).then_some(Self(count))
    }
}

#[cfg(test)]
#[path = "helper_windows_tests.rs"]
mod tests;
