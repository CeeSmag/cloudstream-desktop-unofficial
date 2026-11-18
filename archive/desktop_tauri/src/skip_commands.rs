//! Tauri command that finds where the intro and credits of the playing video are.

use std::sync::Arc;

use serde::Deserialize;
use skip_times::{ExternalIds, SkipLookup, SkipQuery, SkipSegment, TitleKind};
use tauri::State;

use crate::app_state::AppState;

/// What the player page knows about the video that just started.
#[derive(Debug, Clone, Deserialize)]
pub struct SkipRequestDto {
    /// IMDb id of the title, when the provider reports one.
    pub imdb_id: Option<String>,
    /// TheMovieDB id of the title.
    pub tmdb_id: Option<String>,
    /// MyAnimeList id of the title.
    pub mal_id: Option<String>,
    /// The provider's kind of title, such as `Anime` or `TvSeries`.
    pub media_type: Option<String>,
    /// Whether the video is a film (a title without episodes).
    pub is_movie: bool,
    /// Season number of the episode.
    pub season: Option<i32>,
    /// Episode number within the season.
    pub episode: Option<i32>,
    /// Length of the video in milliseconds.
    pub duration_ms: i64,
}

/// Film, series or anime, from the provider's kind of title.
fn title_kind(media_type: Option<&str>, is_movie: bool) -> TitleKind {
    if is_movie {
        return TitleKind::Movie;
    }
    let kind = media_type.unwrap_or_default().to_ascii_lowercase();
    if kind.contains("anime") || kind.contains("ova") {
        TitleKind::Anime
    } else {
        TitleKind::Series
    }
}

fn build_query(request: &SkipRequestDto) -> SkipQuery {
    SkipQuery {
        ids: ExternalIds::new(
            request.imdb_id.as_deref(),
            request.tmdb_id.as_deref(),
            request.mal_id.as_deref(),
        ),
        kind: title_kind(request.media_type.as_deref(), request.is_movie),
        season: request.season.and_then(|n| u32::try_from(n).ok()),
        episode: request.episode.and_then(|n| u32::try_from(n).ok()),
        duration_ms: u64::try_from(request.duration_ms).unwrap_or(0),
    }
}

/// The intro, recap and credits of a video, from the online databases. Empty when the viewer
/// switched the lookup off, the title has no ids, nobody knows, or the databases cannot be reached:
/// skipping is a convenience and never an error.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(episode = ?request.episode), err)]
pub async fn get_skip_segments(
    request: SkipRequestDto,
    state: State<'_, AppState>,
    lookup: State<'_, Arc<SkipLookup>>,
) -> Result<Vec<SkipSegment>, String> {
    if !state.read().settings.skip_lookup().unwrap_or(true) {
        return Ok(Vec::new());
    }
    let query = build_query(&request);
    let lookup = Arc::clone(&lookup);
    let found = tauri::async_runtime::spawn_blocking(move || lookup.find(&query))
        .await
        .map_err(|error| format!("The skip time lookup stopped: {error}"))?;
    Ok(found.unwrap_or_else(|error| {
        tracing::warn!(%error, "no skip times this time");
        Vec::new()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SkipRequestDto {
        SkipRequestDto {
            imdb_id: Some("tt18546730".to_owned()),
            tmdb_id: None,
            mal_id: Some("40748".to_owned()),
            media_type: Some("TvSeries".to_owned()),
            is_movie: false,
            season: Some(2),
            episode: Some(5),
            duration_ms: 1_440_000,
        }
    }

    #[test]
    fn the_kind_of_title_comes_from_the_provider_and_a_film_is_always_a_film() {
        assert_eq!(title_kind(Some("Anime"), false), TitleKind::Anime);
        assert_eq!(title_kind(Some("OVA"), false), TitleKind::Anime);
        assert_eq!(title_kind(Some("TvSeries"), false), TitleKind::Series);
        assert_eq!(title_kind(None, false), TitleKind::Series);
        assert_eq!(title_kind(Some("Anime"), true), TitleKind::Movie);
    }

    #[test]
    fn the_query_carries_checked_ids_and_non_negative_numbers() {
        let query = build_query(&request());
        assert_eq!(query.ids.imdb.as_deref(), Some("tt18546730"));
        assert_eq!((query.season, query.episode), (Some(2), Some(5)));
        assert_eq!(query.duration_ms, 1_440_000);
    }

    #[test]
    fn a_bad_id_or_negative_number_is_dropped_instead_of_sent() {
        let mut bad = request();
        bad.imdb_id = Some("tt1/../x".to_owned());
        bad.season = Some(-1);
        bad.duration_ms = -5;
        let query = build_query(&bad);
        assert_eq!(query.ids.imdb, None);
        assert_eq!(query.season, None);
        assert_eq!(query.duration_ms, 0);
    }
}
