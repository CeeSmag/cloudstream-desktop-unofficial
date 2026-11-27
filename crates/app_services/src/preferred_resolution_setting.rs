//! The preferred playback resolution (Settings > Preferences > "Preferred resolution").

use crate::{ServiceError, SettingsService};

/// Qualities up to this belong to the 1080p group (1080p and lower); anything above to 2160p.
pub const HD_MAX_QUALITY: i32 = 1200;

const PREFERRED_RESOLUTION_KEY: &str = "preferred_resolution";

/// Which resolution group automatic stream selection prefers (largest working file in that group).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferredResolution {
    /// 1080p and lower (`1..=1200`). Default.
    #[default]
    Hd,
    /// Above 1080p: 1440p, 2160p / 4K and higher (`> 1200`).
    Uhd,
    /// No resolution preference: pick the largest working file across all resolutions.
    Any,
}

impl PreferredResolution {
    /// All choices offered in Settings, in display order.
    pub const ALL: [Self; 3] = [Self::Hd, Self::Uhd, Self::Any];

    /// Whether a provider-reported vertical resolution (`quality`) belongs to this preference.
    pub fn matches_quality(self, quality: i32) -> bool {
        match self {
            Self::Any => true,
            Self::Hd => quality > 0 && quality <= HD_MAX_QUALITY,
            Self::Uhd => quality > HD_MAX_QUALITY,
        }
    }

    /// Stable storage key value for persistence.
    pub fn to_text(self) -> &'static str {
        match self {
            Self::Hd => "1080p",
            Self::Uhd => "2160p",
            Self::Any => "any",
        }
    }

    /// Parses a stored setting value back into a [`PreferredResolution`].
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "1080p" | "hd" => Some(Self::Hd),
            "2160p" | "uhd" | "4k" => Some(Self::Uhd),
            "any" => Some(Self::Any),
            _ => None,
        }
    }
}

impl SettingsService {
    /// Which resolution group automatic playback prefers (defaults to 1080p when unset or invalid).
    pub fn preferred_resolution(&self) -> Result<PreferredResolution, ServiceError> {
        Ok(self
            .stored(PREFERRED_RESOLUTION_KEY)?
            .as_deref()
            .and_then(PreferredResolution::parse)
            .unwrap_or_default())
    }

    /// Remembers the viewer's preferred playback resolution.
    pub fn set_preferred_resolution(
        &self,
        resolution: PreferredResolution,
    ) -> Result<(), ServiceError> {
        self.store_value(PREFERRED_RESOLUTION_KEY, resolution.to_text())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    fn service() -> SettingsService {
        let database = Arc::new(Database::open_in_memory().expect("memory db"));
        SettingsService::new(SettingsStore::new(database))
    }

    #[test]
    fn preferred_resolution_defaults_to_1080p_and_round_trips_every_choice() {
        let service = service();
        assert_eq!(
            service.preferred_resolution().expect("default"),
            PreferredResolution::Hd
        );
        for choice in PreferredResolution::ALL {
            service.set_preferred_resolution(choice).expect("save");
            assert_eq!(service.preferred_resolution().expect("read"), choice);
        }
    }

    #[test]
    fn preferred_resolution_garbage_value_falls_back_to_default() {
        let service = service();
        service
            .store_value(PREFERRED_RESOLUTION_KEY, "invalid-value")
            .expect("write");
        assert_eq!(
            service.preferred_resolution().expect("fallback"),
            PreferredResolution::Hd
        );
    }

    #[test]
    fn preferred_resolution_matches_quality_boundaries() {
        let cases = [
            (PreferredResolution::Hd, 0, false),
            (PreferredResolution::Hd, -1, false),
            (PreferredResolution::Hd, 720, true),
            (PreferredResolution::Hd, 1080, true),
            (PreferredResolution::Hd, HD_MAX_QUALITY, true),
            (PreferredResolution::Hd, HD_MAX_QUALITY + 1, false),
            (PreferredResolution::Uhd, 0, false),
            (PreferredResolution::Uhd, HD_MAX_QUALITY, false),
            (PreferredResolution::Uhd, HD_MAX_QUALITY + 1, true),
            (PreferredResolution::Uhd, 1440, true),
            (PreferredResolution::Uhd, 2160, true),
            (PreferredResolution::Any, 0, true),
            (PreferredResolution::Any, 1080, true),
            (PreferredResolution::Any, 2160, true),
        ];
        for (resolution, quality, expected) in cases {
            assert_eq!(resolution.matches_quality(quality), expected);
        }
    }

    #[test]
    fn preferred_resolution_parse_round_trips_and_rejects_unknown() {
        for choice in PreferredResolution::ALL {
            assert_eq!(PreferredResolution::parse(choice.to_text()), Some(choice));
        }
        assert_eq!(
            PreferredResolution::parse(" 2160P "),
            Some(PreferredResolution::Uhd)
        );
        assert_eq!(
            PreferredResolution::parse("4k"),
            Some(PreferredResolution::Uhd)
        );
        assert_eq!(PreferredResolution::parse(""), None);
        assert_eq!(PreferredResolution::parse("720p"), None);
    }
}
