//! The TMDB Discover query for one page of the Browse screen's filters.

use crate::browse_genres::{resolve_genre_param, BrowseFilterRequest, TMDB_API_KEY};

/// The query parameters for TMDB page `tmdb_page` of `request`.
pub(crate) fn build_params(request: &BrowseFilterRequest, tmdb_page: u32) -> Vec<(String, String)> {
    let is_tv = request.media_type.eq_ignore_ascii_case("tv");
    let mut params = vec![
        ("api_key".to_owned(), TMDB_API_KEY.to_owned()),
        ("language".to_owned(), "en-US".to_owned()),
        ("include_adult".to_owned(), "false".to_owned()),
        ("page".to_owned(), tmdb_page.to_string()),
    ];

    if request.sort_by == "rating" {
        params.push(("sort_by".to_owned(), "vote_average.desc".to_owned()));
        params.push(("vote_count.gte".to_owned(), "20".to_owned()));
        if let Some(rating) = request.rating.filter(|&rating| rating > 0.0) {
            params.push(("vote_average.gte".to_owned(), rating.to_string()));
        }
    } else {
        params.push(("sort_by".to_owned(), "popularity.desc".to_owned()));
    }

    append_year_and_genres(&mut params, request, is_tv);
    params
}

fn append_year_and_genres(
    params: &mut Vec<(String, String)>,
    request: &BrowseFilterRequest,
    is_tv: bool,
) {
    if let Some(year) = request.year {
        let key = if is_tv {
            "first_air_date_year"
        } else {
            "primary_release_year"
        };
        params.push((key.to_owned(), year.to_string()));
    }
    if let Some(genres_csv) = resolve_genre_param(&request.effective_genres(), is_tv) {
        params.push(("with_genres".to_owned(), genres_csv));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_params_build_for_movie_multi_genres() {
        let req = BrowseFilterRequest {
            media_type: "movie".to_owned(),
            year: Some(2026),
            sort_by: "popular".to_owned(),
            rating: None,
            genres: vec!["Action".to_owned(), "Science Fiction".to_owned()],
            genre: None,
            page: 1,
        };
        let params = build_params(&req, 1);
        assert!(params.contains(&("sort_by".to_owned(), "popularity.desc".to_owned())));
        assert!(params.contains(&("primary_release_year".to_owned(), "2026".to_owned())));
        assert!(params.contains(&("with_genres".to_owned(), "28,878".to_owned())));
    }

    #[test]
    fn query_params_build_for_tv_rating_and_omit_zero_min_rating() {
        let req = BrowseFilterRequest {
            media_type: "tv".to_owned(),
            year: Some(2024),
            sort_by: "rating".to_owned(),
            rating: Some(8.0),
            genres: vec!["Horror".to_owned(), "Comedy".to_owned()],
            genre: None,
            page: 2,
        };
        let params = build_params(&req, 4);
        assert!(params.contains(&("sort_by".to_owned(), "vote_average.desc".to_owned())));
        assert!(params.contains(&("vote_average.gte".to_owned(), "8".to_owned())));
        assert!(params.contains(&("first_air_date_year".to_owned(), "2024".to_owned())));
        assert!(params.contains(&("with_genres".to_owned(), "9648,35".to_owned())));
    }
}