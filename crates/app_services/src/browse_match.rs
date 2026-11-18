//! Finds the provider's own entry for a title picked on the Browse screen.
//!
//! Browse lists TMDB titles; a provider only knows its own addresses. The provider is searched for
//! the title's name and the result that matches best by name, kind (movie or series) and year is
//! opened. Ported from the former web app (`pickBestSearchMatch`), with one difference: a result
//! whose name does not match at all is not opened (the old app opened the first result anyway,
//! which showed an unrelated title).

use core_model::SearchResult;

use crate::BrowseItem;

/// Points for the same normalised name.
const SAME_NAME: i32 = 100;
/// Points when one name starts with the other ("Fargo" / "Fargo (2014)").
const NAME_PREFIX: i32 = 40;
/// Points for the same kind; the opposite kind costs [`OTHER_KIND`].
const SAME_KIND: i32 = 30;
const OTHER_KIND: i32 = -20;
/// Points for the same year, a year apart, or further apart.
const SAME_YEAR: i32 = 25;
const NEAR_YEAR: i32 = 10;
const OTHER_YEAR: i32 = -15;

/// The best result for `item`, or None when no result's name matches it.
pub fn pick_best_search_match<'a>(
    item: &BrowseItem,
    results: &'a [SearchResult],
) -> Option<&'a SearchResult> {
    let target = normalize_title(&item.name);
    results
        .iter()
        .filter_map(|candidate| name_points(&target, candidate).map(|points| (points, candidate)))
        .map(|(points, candidate)| (points + other_points(item, candidate), candidate))
        // `max_by_key` keeps the last of equal scores; the earlier result should win a tie.
        .fold(
            None,
            |best: Option<(i32, &SearchResult)>, (score, candidate)| match best {
                Some((best_score, _)) if best_score >= score => best,
                _ => Some((score, candidate)),
            },
        )
        .map(|(_, candidate)| candidate)
}

/// Lower case, letters and digits only, single spaces.
fn normalize_title(title: &str) -> String {
    let spaced: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn name_points(target: &str, candidate: &SearchResult) -> Option<i32> {
    let name = normalize_title(&candidate.name);
    if name.is_empty() || target.is_empty() {
        return None;
    }
    if name == target {
        Some(SAME_NAME)
    } else if name.starts_with(target) || target.starts_with(&name) {
        Some(NAME_PREFIX)
    } else {
        None
    }
}

fn other_points(item: &BrowseItem, candidate: &SearchResult) -> i32 {
    kind_points(&item.media_type, candidate.media_type.as_deref())
        + year_points(item.year, candidate.year)
}

fn kind_points(item_kind: &str, candidate_kind: Option<&str>) -> i32 {
    let Some(kind) = candidate_kind.map(str::to_lowercase) else {
        return 0;
    };
    let is_series = ["tv", "series", "anime", "ova"]
        .iter()
        .any(|word| kind.contains(word));
    let is_movie = kind.contains("movie") || kind.contains("film");
    let wants_series = item_kind.eq_ignore_ascii_case("tv");
    match (wants_series, is_series, is_movie) {
        (true, true, _) | (false, _, true) => SAME_KIND,
        (true, false, true) | (false, true, false) => OTHER_KIND,
        _ => 0,
    }
}

fn year_points(item_year: Option<i32>, candidate_year: Option<i32>) -> i32 {
    match (item_year, candidate_year) {
        (Some(wanted), Some(found)) => match (wanted - found).abs() {
            0 => SAME_YEAR,
            1 => NEAR_YEAR,
            _ => OTHER_YEAR,
        },
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browse(name: &str, kind: &str, year: Option<i32>) -> BrowseItem {
        BrowseItem {
            id: format!("{kind}:1"),
            name: name.to_owned(),
            poster_url: None,
            year,
            rating: None,
            media_type: kind.to_owned(),
        }
    }

    fn found(name: &str, url: &str, kind: Option<&str>, year: Option<i32>) -> SearchResult {
        SearchResult {
            name: name.to_owned(),
            url: url.to_owned(),
            api_name: "P".to_owned(),
            media_type: kind.map(str::to_owned),
            poster_url: None,
            year,
            poster_headers: None,
        }
    }

    #[test]
    fn pick_best_search_match_prefers_kind_and_year_over_order() {
        // The web app's test vector: the 1996 film comes first, the 2014 series is wanted.
        let results = [
            found(
                "Fargo",
                "https://example.com/movie/fargo-1996",
                Some("Movie"),
                Some(1996),
            ),
            found(
                "Fargo",
                "https://example.com/tv/fargo-2014",
                Some("TvSeries"),
                Some(2014),
            ),
        ];
        let picked = pick_best_search_match(&browse("Fargo", "tv", Some(2014)), &results);
        assert_eq!(
            picked.map(|r| r.url.as_str()),
            Some("https://example.com/tv/fargo-2014")
        );
    }

    #[test]
    fn pick_best_search_match_ignores_punctuation_and_case() {
        let results = [found("SPIDER-MAN: No Way Home", "u1", None, None)];
        let picked =
            pick_best_search_match(&browse("Spider-Man: No Way Home", "movie", None), &results);
        assert_eq!(picked.map(|r| r.url.as_str()), Some("u1"));
    }

    #[test]
    fn pick_best_search_match_unrelated_names_open_nothing() {
        let results = [found("Something Else", "u1", Some("Movie"), Some(2014))];
        assert!(pick_best_search_match(&browse("Fargo", "movie", Some(2014)), &results).is_none());
        assert!(pick_best_search_match(&browse("Fargo", "movie", None), &[]).is_none());
    }

    #[test]
    fn pick_best_search_match_tie_keeps_provider_order() {
        let results = [
            found("Dune", "first", None, None),
            found("Dune", "second", None, None),
        ];
        let picked = pick_best_search_match(&browse("Dune", "movie", None), &results);
        assert_eq!(picked.map(|r| r.url.as_str()), Some("first"));
    }

    #[test]
    fn pick_best_search_match_prefix_name_counts() {
        let results = [found("Dune: Part Two", "u1", Some("Movie"), Some(2024))];
        let picked = pick_best_search_match(&browse("Dune", "movie", Some(2024)), &results);
        assert_eq!(picked.map(|r| r.url.as_str()), Some("u1"));
    }
}
