//! Browse's filters: movies or shows, a year, popular or best rated, genres, and pages.
//!
//! Ported from the old app (`browseFilters.ts`, `BrowseFilterBar.tsx`); the request goes to
//! `app_services::BrowseService` (TMDB discover).

use app_services::BrowseFilterRequest;

/// The genres Browse offers (TMDB's movie genres; the service maps them for shows).
pub(crate) const GENRES: [&str; 19] = [
    "Action",
    "Adventure",
    "Animation",
    "Comedy",
    "Crime",
    "Documentary",
    "Drama",
    "Family",
    "Fantasy",
    "History",
    "Horror",
    "Music",
    "Mystery",
    "Romance",
    "Science Fiction",
    "Thriller",
    "TV Movie",
    "War",
    "Western",
];

/// The oldest year the year list offers.
pub(crate) const OLDEST_YEAR: i32 = 1970;
/// The year list's first entry: no year filter.
pub(crate) const ANY_YEAR: &str = "Any year";
/// The sort list, in order: popular, then best rated from a minimum rating up.
pub(crate) const SORT_LABELS: [&str; 4] = ["Popular", "Rated 6+", "Rated 7+", "Rated 8+"];

/// What Browse shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BrowseFilters {
    /// `movie` or `tv`.
    pub(crate) kind: String,
    pub(crate) year: Option<i32>,
    /// Best rated from this rating (out of 10) up; None for popular.
    pub(crate) min_rating: Option<u8>,
    pub(crate) genres: Vec<String>,
}

impl Default for BrowseFilters {
    fn default() -> Self {
        Self {
            kind: "movie".to_owned(),
            year: None,
            min_rating: None,
            genres: Vec::new(),
        }
    }
}

impl BrowseFilters {
    /// The service request for page `page` (1-based).
    pub(crate) fn request(&self, page: u32) -> BrowseFilterRequest {
        BrowseFilterRequest {
            media_type: self.kind.clone(),
            year: self.year,
            sort_by: if self.min_rating.is_some() {
                "rating".into()
            } else {
                "popular".into()
            },
            rating: self.min_rating.map(f32::from),
            genres: self.genres.clone(),
            genre: None,
            page,
        }
    }

    /// Adds `genre`, or takes it away when it was chosen.
    pub(crate) fn toggle_genre(&mut self, genre: &str) {
        match self.genres.iter().position(|chosen| chosen == genre) {
            Some(at) => {
                self.genres.remove(at);
            }
            None => self.genres.push(genre.to_owned()),
        }
    }
}

/// "Any year", then `current_year` down to [`OLDEST_YEAR`].
pub(crate) fn year_labels(current_year: i32) -> Vec<String> {
    let mut labels = vec![ANY_YEAR.to_owned()];
    labels.extend(
        (OLDEST_YEAR..=current_year)
            .rev()
            .map(|year| year.to_string()),
    );
    labels
}

/// The year a year-list label names; None for "Any year".
pub(crate) fn year_from_label(label: &str) -> Option<i32> {
    label.parse().ok()
}

/// The minimum rating a sort label names; None for popular.
pub(crate) fn rating_from_label(label: &str) -> Option<u8> {
    label
        .strip_prefix("Rated ")
        .and_then(|rest| rest.strip_suffix('+'))
        .and_then(|number| number.parse().ok())
}

/// The calendar year (UTC) of a Unix time in milliseconds.
pub(crate) fn year_of_unix_ms(ms: i64) -> i32 {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let days = ms.div_euclid(86_400_000) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let year = year_of_era + era * 400 + i64::from(month_index >= 10);
    i32::try_from(year).unwrap_or(OLDEST_YEAR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_carries_kind_year_sort_rating_genres_and_page() {
        let mut filters = BrowseFilters {
            kind: "tv".into(),
            year: Some(2024),
            ..BrowseFilters::default()
        };
        filters.toggle_genre("Drama");
        filters.toggle_genre("Crime");
        let popular = filters.request(1);
        assert_eq!(
            (
                popular.media_type.as_str(),
                popular.year,
                popular.sort_by.as_str()
            ),
            ("tv", Some(2024), "popular")
        );
        assert_eq!(popular.rating, None);
        assert_eq!(popular.genres, ["Drama", "Crime"]);
        filters.min_rating = Some(7);
        let rated = filters.request(3);
        assert_eq!(
            (rated.sort_by.as_str(), rated.rating, rated.page),
            ("rating", Some(7.0), 3)
        );
        filters.toggle_genre("Drama");
        assert_eq!(filters.genres, ["Crime"], "a second press takes it away");
    }

    #[test]
    fn year_labels_and_their_years() {
        let labels = year_labels(2026);
        assert_eq!(labels.first().map(String::as_str), Some(ANY_YEAR));
        assert_eq!(labels.get(1).map(String::as_str), Some("2026"));
        assert_eq!(labels.last().map(String::as_str), Some("1970"));
        assert_eq!(year_from_label("2001"), Some(2001));
        assert_eq!(year_from_label(ANY_YEAR), None);
    }

    #[test]
    fn rating_from_label_reads_the_sort_list() {
        assert_eq!(rating_from_label("Popular"), None);
        assert_eq!(rating_from_label("Rated 7+"), Some(7));
        assert!(SORT_LABELS
            .iter()
            .skip(1)
            .all(|label| rating_from_label(label).is_some()));
    }

    #[test]
    fn year_of_unix_ms_handles_new_year_and_leap_days() {
        assert_eq!(year_of_unix_ms(0), 1970);
        assert_eq!(year_of_unix_ms(951_782_400_000), 2000, "2000-02-29");
        assert_eq!(
            year_of_unix_ms(1_767_225_599_000),
            2025,
            "2025-12-31 23:59:59"
        );
        assert_eq!(year_of_unix_ms(1_767_225_600_000), 2026, "2026-01-01");
    }
}
