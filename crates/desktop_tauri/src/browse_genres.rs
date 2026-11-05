//! Genre mapping and DTO conversion for TMDB discovery.

use serde::{Deserialize, Serialize};

/// Public v3 API key used for TMDB discovery queries.
pub const TMDB_API_KEY: &str = "e6333b32409e02a4a6eba6fb7ff866bb";
/// Base URL of the TMDB v3 REST API.
pub const TMDB_BASE_URL: &str = "https://api.themoviedb.org/3";
const TMDB_POSTER_BASE: &str = "https://image.tmdb.org/t/p/w342";

const MOVIE_GENRES: &[(&str, u32)] = &[
    ("action", 28),
    ("adventure", 12),
    ("animation", 16),
    ("comedy", 35),
    ("crime", 80),
    ("documentary", 99),
    ("drama", 18),
    ("family", 10751),
    ("fantasy", 14),
    ("history", 36),
    ("horror", 27),
    ("music", 10402),
    ("mystery", 9648),
    ("romance", 10749),
    ("science fiction", 878),
    ("thriller", 53),
    ("tv movie", 10770),
    ("war", 10752),
    ("western", 37),
];

const TV_GENRES: &[(&str, u32)] = &[
    ("action", 10759),
    ("adventure", 10759),
    ("animation", 16),
    ("comedy", 35),
    ("crime", 80),
    ("documentary", 99),
    ("drama", 18),
    ("family", 10751),
    ("fantasy", 10765),
    ("history", 10768),
    ("horror", 9648),
    ("music", 10751),
    ("mystery", 9648),
    ("romance", 18),
    ("science fiction", 10765),
    ("thriller", 9648),
    ("tv movie", 18),
    ("war", 10768),
    ("western", 37),
];

/// Filters requested by the Browse screen.
#[derive(Debug, Clone, Deserialize)]
pub struct BrowseFilterRequest {
    /// Media category: `"movie"` or `"tv"`.
    #[serde(alias = "mediaType")]
    pub media_type: String,
    /// Release or first-air year if specified.
    pub year: Option<i32>,
    /// Sort mode: `"popular"` or `"rating"`.
    #[serde(alias = "sortBy")]
    pub sort_by: String,
    /// Minimum rating threshold (`0.0` to `10.0`).
    pub rating: Option<f32>,
    /// Selected genre names (supports multi-genre filtering).
    #[serde(default)]
    pub genres: Vec<String>,
    /// Optional single genre name for backward compatibility.
    #[serde(default)]
    pub genre: Option<String>,
    /// 1-based page index (60 items per page).
    pub page: u32,
}

impl BrowseFilterRequest {
    /// Returns the list of active genre names from `genres` or fallback `genre`.
    pub fn effective_genres(&self) -> Vec<String> {
        if !self.genres.is_empty() {
            return self.genres.clone();
        }
        self.genre.iter().cloned().collect()
    }
}

/// A media item returned for display on the Browse screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowseItemDto {
    /// Unique identifier (`movie:<id>` or `tv:<id>`).
    pub id: String,
    /// Display title.
    pub name: String,
    /// Full poster image URL if available.
    pub poster_url: Option<String>,
    /// Release or first-air year.
    pub year: Option<i32>,
    /// Average rating rounded to 1 decimal place.
    pub rating: Option<f32>,
    /// Media category (`"movie"` or `"tv"`).
    pub media_type: String,
}

/// Paginated result of browse items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowsePageDto {
    /// Cards for the current browse page (up to 60).
    pub items: Vec<BrowseItemDto>,
    /// Current 1-based browse page.
    pub page: u32,
    /// Total available browse pages.
    pub total_pages: u32,
}

/// Raw item entry returned by TMDB's `/discover` endpoint.
#[derive(Debug, Deserialize)]
pub struct TmdbRawItem {
    id: Option<i64>,
    title: Option<String>,
    original_title: Option<String>,
    name: Option<String>,
    original_name: Option<String>,
    poster_path: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    vote_average: Option<f64>,
}

/// Raw paginated response returned by TMDB's `/discover` endpoint.
#[derive(Debug, Deserialize)]
pub struct TmdbPageResponse {
    /// Raw items on this TMDB page.
    pub results: Option<Vec<TmdbRawItem>>,
    /// Total TMDB pages matching the query.
    pub total_pages: Option<u32>,
}

/// Resolves a genre name to its TMDB movie genre ID.
pub fn movie_genre_id(name: &str) -> Option<u32> {
    let key = name.trim().to_ascii_lowercase();
    MOVIE_GENRES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, id)| *id)
}

/// Resolves a genre name to a valid TMDB TV genre ID.
pub fn tv_genre_id(name: &str) -> Option<u32> {
    let key = name.trim().to_ascii_lowercase();
    TV_GENRES.iter().find(|(k, _)| *k == key).map(|(_, id)| *id)
}

/// Resolves multiple genre names into a deduplicated comma-separated `with_genres` query value.
pub fn resolve_genre_param(genres: &[String], is_tv: bool) -> Option<String> {
    let mut ids: Vec<u32> = Vec::new();
    for g in genres {
        let resolved = if is_tv {
            tv_genre_id(g)
        } else {
            movie_genre_id(g)
        };
        if let Some(id) = resolved {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    if ids.is_empty() {
        return None;
    }
    let joined = ids
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    Some(joined)
}

/// Extracts the 4-digit year prefix from an ISO date string (`YYYY-MM-DD`).
pub fn extract_year(date_str: Option<&str>) -> Option<i32> {
    let raw = date_str?.trim();
    raw.get(..4).and_then(|y| y.parse::<i32>().ok())
}

/// Converts a raw TMDB item into a [`BrowseItemDto`], returning `None` if id or title is missing.
pub fn convert_raw_item(raw: TmdbRawItem, default_type: &str) -> Option<BrowseItemDto> {
    let id = raw.id?;
    let name = raw
        .title
        .or(raw.original_title)
        .or(raw.name)
        .or(raw.original_name)?;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let date = raw
        .release_date
        .as_deref()
        .or(raw.first_air_date.as_deref());
    let poster_url = raw
        .poster_path
        .filter(|p| !p.trim().is_empty())
        .map(|p| format!("{TMDB_POSTER_BASE}{p}"));
    let rating = raw.vote_average.map(|v| (v * 10.0).round() as f32 / 10.0);

    Some(BrowseItemDto {
        id: format!("{default_type}:{id}"),
        name: trimmed.to_owned(),
        poster_url,
        year: extract_year(date),
        rating,
        media_type: default_type.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movie_and_tv_genres_map_all_19_genres() {
        let valid_tv_ids = [10759, 16, 35, 80, 99, 18, 10751, 10765, 10768, 9648, 37];
        for (name, _) in MOVIE_GENRES {
            assert!(movie_genre_id(name).is_some());
            let tv_id = tv_genre_id(name).expect("valid tv genre id");
            assert!(valid_tv_ids.contains(&tv_id));
        }
        assert_eq!(movie_genre_id("Nonexistent"), None);
        assert_eq!(tv_genre_id("Nonexistent"), None);
    }

    #[test]
    fn resolve_genre_param_joins_and_deduplicates_multiple_genres() {
        let genres = vec![
            "Action".to_owned(),
            "Adventure".to_owned(),
            "Comedy".to_owned(),
        ];
        assert_eq!(
            resolve_genre_param(&genres, false),
            Some("28,12,35".to_owned())
        );
        assert_eq!(
            resolve_genre_param(&genres, true),
            Some("10759,35".to_owned())
        );
        assert_eq!(resolve_genre_param(&[], false), None);
    }

    #[test]
    fn raw_movie_and_tv_items_convert_to_dto() {
        let movie: TmdbRawItem = serde_json::from_str(
            r#"{"id":155,"title":"The Dark Knight","poster_path":"/p.jpg","release_date":"2008-07-16","vote_average":8.54}"#,
        )
        .expect("valid json");
        let dto = convert_raw_item(movie, "movie").expect("valid movie dto");
        assert_eq!(dto.id, "movie:155");
        assert_eq!(dto.year, Some(2008));
        assert_eq!(dto.rating, Some(8.5));

        let tv: TmdbRawItem = serde_json::from_str(
            r#"{"id":5920,"name":"The Mentalist","first_air_date":"2008-09-23","vote_average":8.42}"#,
        )
        .expect("valid json");
        let tv_dto = convert_raw_item(tv, "tv").expect("valid tv dto");
        assert_eq!(tv_dto.id, "tv:5920");
    }

    #[test]
    fn browse_filter_request_deserializes_multi_and_single_genres() {
        let multi = r#"{"media_type":"movie","year":2026,"sort_by":"popular","rating":null,"genres":["Action","Comedy"],"page":1}"#;
        let req_multi: BrowseFilterRequest = serde_json::from_str(multi).expect("valid multi");
        assert_eq!(req_multi.effective_genres(), vec!["Action", "Comedy"]);

        let single = r#"{"mediaType":"tv","year":2024,"sortBy":"rating","rating":7.5,"genre":"Drama","page":2}"#;
        let req_single: BrowseFilterRequest = serde_json::from_str(single).expect("valid single");
        assert_eq!(req_single.effective_genres(), vec!["Drama"]);
    }
}
