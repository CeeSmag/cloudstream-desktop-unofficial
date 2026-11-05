//! Tauri commands for browsing catalog by media type, year, rating/popularity, and genres.

use std::collections::HashSet;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use crate::browse_genres::{
    convert_raw_item, resolve_genre_param, BrowseFilterRequest, BrowseItemDto, BrowsePageDto,
    TmdbPageResponse, TmdbRawItem, TMDB_API_KEY, TMDB_BASE_URL,
};

const TMDB_PAGES_PER_BROWSE_PAGE: u32 = 3;
/// TMDB Discover API returns HTTP 400 for any `page > 500`.
const MAX_TMDB_PAGES: u32 = 500;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const USER_AGENT: &str = "CloudStream-rs";

/// Fetches a single TMDB Discover page.
pub trait BrowseFetcher: Send + Sync {
    /// Sends a GET request to `endpoint` with `params` and parses the JSON page response.
    fn fetch_page(
        &self,
        endpoint: &str,
        params: &[(String, String)],
    ) -> Result<TmdbPageResponse, String>;
}

/// Connection-pooled `ureq` implementation of [`BrowseFetcher`].
pub struct UreqBrowseFetcher {
    agent: ureq::Agent,
}

impl Default for UreqBrowseFetcher {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }
}

fn redact_key(message: &str) -> String {
    message.replace(TMDB_API_KEY, "[REDACTED]")
}

impl BrowseFetcher for UreqBrowseFetcher {
    fn fetch_page(
        &self,
        endpoint: &str,
        params: &[(String, String)],
    ) -> Result<TmdbPageResponse, String> {
        let mut req = self.agent.get(endpoint).header("User-Agent", USER_AGENT);
        for (k, v) in params {
            req = req.query(k, v);
        }
        let res = req
            .call()
            .map_err(|e| format!("HTTP request failed: {}", redact_key(&e.to_string())))?;
        let status = res.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("TMDB returned HTTP {status}"));
        }
        let mut body = String::new();
        res.into_body()
            .into_reader()
            .take(MAX_RESPONSE_BYTES)
            .read_to_string(&mut body)
            .map_err(|e| format!("Reading response body failed: {e}"))?;
        serde_json::from_str(&body).map_err(|e| format!("JSON decode failed: {e}"))
    }
}

/// Coordinates multi-page TMDB discovery aggregation for the Browse screen.
pub struct BrowseClient {
    fetcher: Arc<dyn BrowseFetcher>,
}

impl Default for BrowseClient {
    fn default() -> Self {
        Self::new(Arc::new(UreqBrowseFetcher::default()))
    }
}

impl BrowseClient {
    /// Creates a new [`BrowseClient`] backed by `fetcher`.
    pub fn new(fetcher: Arc<dyn BrowseFetcher>) -> Self {
        Self { fetcher }
    }

    /// Fetches up to 3 TMDB pages (60 items) for `request`, deduplicating items by ID.
    pub fn fetch_browse_page(
        &self,
        request: &BrowseFilterRequest,
    ) -> Result<BrowsePageDto, String> {
        let max_browse_page = MAX_TMDB_PAGES.div_ceil(TMDB_PAGES_PER_BROWSE_PAGE);
        let safe_page = request.page.clamp(1, max_browse_page);
        let start_page = (safe_page - 1) * TMDB_PAGES_PER_BROWSE_PAGE + 1;

        let (items, total_tmdb_pages) = self.collect_batch(request, start_page)?;

        let total_browse_pages = total_tmdb_pages.div_ceil(TMDB_PAGES_PER_BROWSE_PAGE).max(1);
        Ok(BrowsePageDto {
            items,
            page: safe_page.min(total_browse_pages),
            total_pages: total_browse_pages,
        })
    }

    fn collect_batch(
        &self,
        request: &BrowseFilterRequest,
        start_page: u32,
    ) -> Result<(Vec<BrowseItemDto>, u32), String> {
        let is_tv = request.media_type.eq_ignore_ascii_case("tv");
        let media_label = if is_tv { "tv" } else { "movie" };
        let endpoint = format!("{TMDB_BASE_URL}/discover/{media_label}");
        let mut all_items = Vec::with_capacity(60);
        let mut seen_ids = HashSet::with_capacity(60);
        let mut total_tmdb_pages = MAX_TMDB_PAGES;

        for offset in 0..TMDB_PAGES_PER_BROWSE_PAGE {
            let page_num = start_page + offset;
            if page_num > total_tmdb_pages || page_num > MAX_TMDB_PAGES {
                break;
            }
            let params = build_params(request, page_num);
            match self.fetcher.fetch_page(&endpoint, &params) {
                Ok(page_res) => {
                    total_tmdb_pages = page_res
                        .total_pages
                        .map_or(total_tmdb_pages, |tp| tp.min(MAX_TMDB_PAGES));
                    append_unique_items(
                        &mut all_items,
                        &mut seen_ids,
                        page_res.results,
                        media_label,
                    );
                }
                Err(err) if offset == 0 => return Err(err),
                Err(err) => {
                    tracing::warn!(page_num, %err, "secondary browse page fetch failed; returning partial items");
                    break;
                }
            }
        }
        Ok((all_items, total_tmdb_pages))
    }
}

fn append_unique_items(
    dest: &mut Vec<BrowseItemDto>,
    seen_ids: &mut HashSet<String>,
    raw_items: Option<Vec<TmdbRawItem>>,
    media_label: &str,
) {
    let dtos = raw_items
        .into_iter()
        .flatten()
        .filter_map(|item| convert_raw_item(item, media_label));
    for dto in dtos {
        if seen_ids.insert(dto.id.clone()) {
            dest.push(dto);
        }
    }
}

fn build_params(req: &BrowseFilterRequest, tmdb_page: u32) -> Vec<(String, String)> {
    let is_tv = req.media_type.eq_ignore_ascii_case("tv");
    let mut params = vec![
        ("api_key".to_owned(), TMDB_API_KEY.to_owned()),
        ("language".to_owned(), "en-US".to_owned()),
        ("include_adult".to_owned(), "false".to_owned()),
        ("page".to_owned(), tmdb_page.to_string()),
    ];

    if req.sort_by == "rating" {
        params.push(("sort_by".to_owned(), "vote_average.desc".to_owned()));
        params.push(("vote_count.gte".to_owned(), "20".to_owned()));
        if let Some(r) = req.rating.filter(|&r| r > 0.0) {
            params.push(("vote_average.gte".to_owned(), r.to_string()));
        }
    } else {
        params.push(("sort_by".to_owned(), "popularity.desc".to_owned()));
    }

    append_year_and_genres(&mut params, req, is_tv);
    params
}

fn append_year_and_genres(
    params: &mut Vec<(String, String)>,
    req: &BrowseFilterRequest,
    is_tv: bool,
) {
    if let Some(y) = req.year {
        let key = if is_tv {
            "first_air_date_year"
        } else {
            "primary_release_year"
        };
        params.push((key.to_owned(), y.to_string()));
    }
    if let Some(genres_csv) = resolve_genre_param(&req.effective_genres(), is_tv) {
        params.push(("with_genres".to_owned(), genres_csv));
    }
}

/// Tauri command that queries TMDB Discover API to return up to 60 items per browse page.
#[tauri::command]
#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(
        page = request.page,
        media_type = %request.media_type,
        year = ?request.year,
        sort_by = %request.sort_by,
        genres = ?request.genres
    ),
    err
)]
pub async fn browse_catalog(
    client: tauri::State<'_, Arc<BrowseClient>>,
    request: BrowseFilterRequest,
) -> Result<BrowsePageDto, String> {
    let client = Arc::clone(&client);
    tauri::async_runtime::spawn_blocking(move || client.fetch_browse_page(&request))
        .await
        .map_err(|e| format!("Worker thread error: {e}"))?
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct FakeBrowseFetcher {
        calls: Mutex<Vec<u32>>,
        responses: Mutex<Vec<Result<TmdbPageResponse, String>>>,
    }

    impl FakeBrowseFetcher {
        fn new(responses: Vec<Result<TmdbPageResponse, String>>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
            }
        }
    }

    impl BrowseFetcher for FakeBrowseFetcher {
        fn fetch_page(
            &self,
            _endpoint: &str,
            params: &[(String, String)],
        ) -> Result<TmdbPageResponse, String> {
            let page = params
                .iter()
                .find(|(k, _)| k == "page")
                .and_then(|(_, v)| v.parse::<u32>().ok())
                .unwrap_or(0);
            self.calls.lock().expect("lock").push(page);
            self.responses.lock().expect("lock").remove(0)
        }
    }

    fn raw_item(id: i64, title: &str) -> TmdbRawItem {
        serde_json::from_str(&format!(
            r#"{{"id":{id},"title":"{title}","release_date":"2026-01-01","vote_average":7.5}}"#
        ))
        .expect("valid item json")
    }

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

    #[test]
    fn stops_fetching_when_total_pages_reached_and_deduplicates() {
        let fetcher = Arc::new(FakeBrowseFetcher::new(vec![
            Ok(TmdbPageResponse {
                results: Some(vec![raw_item(1, "Alpha"), raw_item(2, "Beta")]),
                total_pages: Some(2),
            }),
            Ok(TmdbPageResponse {
                results: Some(vec![raw_item(2, "Beta"), raw_item(3, "Gamma")]),
                total_pages: Some(2),
            }),
        ]));
        let client = BrowseClient::new(fetcher.clone());
        let req = BrowseFilterRequest {
            media_type: "movie".to_owned(),
            year: Some(2026),
            sort_by: "popular".to_owned(),
            rating: None,
            genres: Vec::new(),
            genre: None,
            page: 1,
        };
        let res = client.fetch_browse_page(&req).expect("succeeds");
        assert_eq!(*fetcher.calls.lock().expect("lock"), vec![1, 2]);
        assert_eq!(res.items.len(), 3);
        assert_eq!(res.total_pages, 1);
    }

    #[test]
    fn partial_page_failure_returns_collected_items_and_clamps_500_pages() {
        let fetcher = Arc::new(FakeBrowseFetcher::new(vec![
            Ok(TmdbPageResponse {
                results: Some(vec![raw_item(10, "First")]),
                total_pages: Some(9000),
            }),
            Err("TMDB returned HTTP 502".to_owned()),
        ]));
        let client = BrowseClient::new(fetcher);
        let req = BrowseFilterRequest {
            media_type: "movie".to_owned(),
            year: None,
            sort_by: "popular".to_owned(),
            rating: None,
            genres: Vec::new(),
            genre: None,
            page: 1,
        };
        let res = client.fetch_browse_page(&req).expect("partial success");
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.total_pages, 167);
        assert!(redact_key(&format!("key={TMDB_API_KEY}")).contains("[REDACTED]"));
    }
}
