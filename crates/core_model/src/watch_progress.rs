use serde::{Deserialize, Serialize};

/// How far the user got in one playable item.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchProgress {
    /// Provider the item belongs to.
    pub provider: String,
    /// Provider payload identifying the item (movie data URL or episode data).
    pub media_key: String,
    /// Title shown in "continue watching".
    pub title: String,
    /// Provider URL of the parent title, used to reopen its details page.
    #[serde(default)]
    pub detail_url: String,
    /// Poster image URL.
    #[serde(default)]
    pub poster_url: Option<String>,
    /// Season number when known.
    #[serde(default)]
    pub season: Option<i32>,
    /// Episode number when known.
    #[serde(default)]
    pub episode: Option<i32>,
    /// Playback position in milliseconds.
    pub position_ms: i64,
    /// Total length in milliseconds, 0 when unknown.
    pub duration_ms: i64,
    /// When the progress was last saved, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub updated_at_ms: i64,
}

impl WatchProgress {
    /// Fraction watched in `0.0..=1.0`, or `0.0` when the length is unknown.
    pub fn fraction_watched(&self) -> f32 {
        if self.duration_ms <= 0 {
            return 0.0;
        }
        (self.position_ms as f32 / self.duration_ms as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(position_ms: i64, duration_ms: i64) -> WatchProgress {
        WatchProgress {
            position_ms,
            duration_ms,
            ..WatchProgress::default()
        }
    }

    #[test]
    fn fraction_watched_is_zero_when_duration_unknown() {
        assert_eq!(progress(5_000, 0).fraction_watched(), 0.0);
    }

    #[test]
    fn fraction_watched_is_clamped_to_one() {
        assert_eq!(progress(20_000, 10_000).fraction_watched(), 1.0);
    }

    #[test]
    fn fraction_watched_reports_halfway() {
        assert_eq!(progress(5_000, 10_000).fraction_watched(), 0.5);
    }
}
