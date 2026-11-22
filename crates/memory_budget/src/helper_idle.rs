//! How long the Cloudflare verification helper may sit idle before it is closed.

use std::time::Duration;

/// Seconds in a minute.
const SECONDS_PER_MINUTE: u64 = 60;

/// The helper is a browser engine of 100-300 MB; it starts again at the next check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperIdle {
    /// Closed after this many idle minutes.
    After(u8),
    /// Kept until the app closes.
    Never,
}

impl HelperIdle {
    /// The values the settings screen offers.
    pub const CHOICES: [HelperIdle; 5] = [
        Self::After(1),
        Self::After(2),
        Self::After(5),
        Self::After(15),
        Self::Never,
    ];

    /// The idle time in the form the watcher takes (`None` = never).
    pub fn as_duration(self) -> Option<Duration> {
        match self {
            Self::After(minutes) => {
                Some(Duration::from_secs(u64::from(minutes) * SECONDS_PER_MINUTE))
            }
            Self::Never => None,
        }
    }

    /// The text kept in the settings database.
    pub fn to_text(self) -> String {
        match self {
            Self::After(minutes) => minutes.to_string(),
            Self::Never => "never".to_owned(),
        }
    }

    /// Reads [`Self::to_text`] back; anything else (including 0 minutes) is not accepted.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("never") {
            return Some(Self::Never);
        }
        text.parse::<u8>()
            .ok()
            .filter(|minutes| *minutes > 0)
            .map(Self::After)
    }
}

#[cfg(test)]
#[path = "helper_idle_tests.rs"]
mod tests;
