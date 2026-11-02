//! Data transfer objects exchanged between Rust backend and frontend.

use serde::{Deserialize, Serialize};

/// Playback request sent from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayRequestDto {
    /// Provider identifying which extension owns this stream.
    pub provider: String,
    /// Direct stream key or episode data payload.
    pub media_key: String,
    /// Human readable title.
    pub title: String,
    /// Detail URL of the media.
    pub detail_url: Option<String>,
    /// Poster image URL.
    pub poster_url: Option<String>,
}

/// Snapshot of the video player state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaybackStatusDto {
    /// Playback state ("Idle", "Loading", "Playing", "Paused", "Ended", "Failed").
    pub state: String,
    /// Current position in milliseconds.
    pub position_ms: i64,
    /// Total duration in milliseconds.
    pub duration_ms: i64,
    /// True when video stream is present.
    pub has_video: bool,
    /// Current volume (0 to 100).
    pub volume: f64,
    /// Audio and subtitle tracks.
    pub tracks: Vec<MediaTrackDto>,
}

/// Audio or subtitle track info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaTrackDto {
    /// Numeric ID used by mpv.
    pub id: i64,
    /// "audio" or "subtitle".
    pub kind: String,
    /// Track label or title.
    pub title: String,
    /// Optional ISO language code.
    pub language: Option<String>,
    /// True if currently selected.
    pub selected: bool,
}

/// A media item in a shelf or carousel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeItemDto {
    /// Unique identifier / URL.
    pub id: String,
    /// Display title name.
    pub name: String,
    /// Media page URL.
    pub url: String,
    /// Optional poster image URL.
    pub poster_url: Option<String>,
    /// Provider that produced this item.
    pub provider: String,
    /// Media type (Movie, TvSeries, etc.).
    pub media_type: Option<String>,
    /// Optional release year.
    pub year: Option<i32>,
}

/// Shelf row of home items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeSectionDto {
    /// Section heading (e.g. "Trending", "Popular Movies").
    pub title: String,
    /// List of media items in this section.
    pub items: Vec<HomeItemDto>,
    /// Whether the section uses wide 16:9 artwork.
    pub is_horizontal: bool,
}

/// Item returned from search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchItemDto {
    /// Identifier / URL.
    pub id: String,
    /// Item title.
    pub name: String,
    /// Page URL.
    pub url: String,
    /// Poster image URL.
    pub poster_url: Option<String>,
    /// Provider origin.
    pub provider: String,
    /// Optional year.
    pub year: Option<i32>,
}

/// Detailed media information with synopsis and episode listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDetailsDto {
    /// Title name.
    pub name: String,
    /// Source URL.
    pub url: String,
    /// Owning provider.
    pub provider: String,
    /// Movie, TvSeries, etc.
    pub media_type: Option<String>,
    /// Data URL for movies.
    pub data_url: Option<String>,
    /// Vertical poster image.
    pub poster_url: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Synopsis.
    pub plot: Option<String>,
    /// Genres or tags.
    pub tags: Vec<String>,
    /// List of available episodes.
    pub episodes: Vec<EpisodeDto>,
}

/// An episode belonging to a series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeDto {
    /// Episode data key passed to play.
    pub id: String,
    /// Episode payload.
    pub data: String,
    /// Episode name.
    pub name: String,
    /// Season number.
    pub season: Option<i32>,
    /// Episode number.
    pub episode: Option<i32>,
    /// Optional thumbnail URL.
    pub poster_url: Option<String>,
    /// Optional description.
    pub description: Option<String>,
}

/// Library bookmark item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkDto {
    /// Media URL or ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Poster image URL.
    pub poster_url: Option<String>,
    /// Bookmark category ("Watching", "PlanToWatch", "Completed", "Dropped").
    pub status: String,
    /// Source provider.
    pub provider: String,
    /// Source URL.
    pub url: String,
}

/// Progress watch history record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchProgressDto {
    /// Media ID.
    pub media_id: String,
    /// Title.
    pub title: String,
    /// Current position in milliseconds.
    pub position_ms: i64,
    /// Total duration in milliseconds.
    pub duration_ms: i64,
    /// Percentage watched (0.0 to 1.0).
    pub percentage: f32,
    /// Last played timestamp in seconds.
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_request_roundtrip_preserves_fields() {
        let req = PlayRequestDto {
            provider: "TestProvider".to_owned(),
            media_key: "ep-101".to_owned(),
            title: "Episode 1".to_owned(),
            detail_url: Some("https://example.com/show".to_owned()),
            poster_url: Some("https://example.com/img.jpg".to_owned()),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let parsed: PlayRequestDto = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.provider, "TestProvider");
        assert_eq!(parsed.media_key, "ep-101");
        assert_eq!(parsed.title, "Episode 1");
    }

    #[test]
    fn playback_status_default_is_empty() {
        let status = PlaybackStatusDto::default();
        assert_eq!(status.position_ms, 0);
        assert_eq!(status.duration_ms, 0);
        assert!(!status.has_video);
        assert!(status.tracks.is_empty());
    }

    #[test]
    fn home_section_roundtrip() {
        let item = HomeItemDto {
            id: "1".to_owned(),
            name: "Movie".to_owned(),
            url: "https://example.com/1".to_owned(),
            poster_url: None,
            provider: "Provider".to_owned(),
            media_type: Some("Movie".to_owned()),
            year: Some(2025),
        };
        let section = HomeSectionDto {
            title: "Trending".to_owned(),
            items: vec![item],
            is_horizontal: false,
        };
        let json = serde_json::to_string(&section).expect("serialize");
        let parsed: HomeSectionDto = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.title, "Trending");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].name, "Movie");
    }
}
