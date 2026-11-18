//! Data transfer objects exchanged between Rust backend and frontend.

use serde::{Deserialize, Serialize};

/// Next episode payload when watching a series.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NextEpisodeDto {
    /// Episode payload.
    pub media_key: String,
    /// Season number.
    pub season: Option<i32>,
    /// Episode number.
    pub episode: Option<i32>,
}

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
    /// Season number, if known.
    #[serde(default)]
    pub season: Option<i32>,
    /// Episode number, if known.
    #[serde(default)]
    pub episode: Option<i32>,
    /// Next episode, if this show has one.
    #[serde(default)]
    pub next_episode: Option<NextEpisodeDto>,
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
    /// Furthest position in milliseconds loaded into the buffer.
    #[serde(default)]
    pub buffered_ms: i64,
    /// True when the player has paused to load/buffer network cache.
    #[serde(default)]
    pub is_buffering: bool,
    /// Buffering completion percentage (0-100) until playback resumes, if known.
    #[serde(default)]
    pub buffer_percent: Option<u32>,
    /// Audio and subtitle tracks.
    pub tracks: Vec<MediaTrackDto>,
    /// Chapters of the stream, in time order; empty when it has none.
    pub chapters: Vec<ChapterDto>,
    /// Why playback failed, when `state` is "Failed".
    pub error: Option<String>,
}

/// One chapter of the stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChapterDto {
    /// Name of the chapter.
    pub title: String,
    /// Where it starts, in milliseconds.
    pub time_ms: i64,
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
    /// Wide background picture.
    pub background_url: Option<String>,
    /// Rating out of 10, when the provider gives one.
    pub rating: Option<f32>,
    /// Running time in minutes.
    pub duration_minutes: Option<i32>,
    /// The cast.
    pub cast: Vec<core_model::CastMember>,
    /// IMDb id of the title, when the provider reports one.
    pub imdb_id: Option<String>,
    /// TheMovieDB id of the title.
    pub tmdb_id: Option<String>,
    /// MyAnimeList id of the title.
    pub mal_id: Option<String>,
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
    /// Provider payload identifying the playable item (episode data or movie URL).
    pub media_id: String,
    /// Provider the item belongs to.
    pub provider: String,
    /// Provider URL of the parent title; empty when it was never recorded.
    pub detail_url: String,
    /// Poster image URL.
    pub poster_url: Option<String>,
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
    /// Season number, if known.
    #[serde(default)]
    pub season: Option<i32>,
    /// Episode number, if known.
    #[serde(default)]
    pub episode: Option<i32>,
}

/// Resume pointer for a show or movie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumePointerDto {
    /// Provider the title belongs to.
    pub provider: String,
    /// Provider URL of the parent title.
    pub detail_url: String,
    /// Episode data key or movie payload.
    pub media_key: String,
    /// Title name.
    pub title: String,
    /// Poster image URL.
    pub poster_url: Option<String>,
    /// Season number, if known.
    pub season: Option<i32>,
    /// Episode number, if known.
    pub episode: Option<i32>,
    /// Current playback position in milliseconds.
    pub position_ms: i64,
    /// Total duration in milliseconds.
    pub duration_ms: i64,
}

#[cfg(test)]
#[path = "dto_tests.rs"]
mod tests;
