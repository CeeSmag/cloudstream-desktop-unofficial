//! T2: replies the real sidecar gave (recorded from the CineStream extension, then trimmed and
//! stripped of addresses) decoded through the gateway, so a change on either side of the protocol
//! that the Rust types cannot read fails here instead of in the app.
//!
//! The fixtures live in `tests/fixtures/sidecar_replies/`; `scripts/record_sidecar_replies.py` records
//! them again from the real sidecar (trimmed, with every address replaced by an `example.test` one).
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use core_model::{LinkSearchCursor, MainPageEntry};
use extension_api::{GatewayError, ProviderGateway};
use extension_runtime::{RpcProviderGateway, RpcTransport};
use serde_json::Value;

/// Answers every call with the next recorded reply, in order, and remembers what was asked.
struct RecordedTransport {
    replies: Mutex<Vec<Value>>,
    asked: Arc<Mutex<Vec<(String, Value)>>>,
}

impl RecordedTransport {
    /// One reply: the whole fixture.
    fn replying_once(fixture: &str) -> Self {
        Self::replying_in_turn(vec![
            serde_json::from_str(fixture).expect("fixture is valid JSON")
        ])
    }

    /// A fixture that is a list of replies, answered one per call.
    fn replying_each(fixture: &str) -> Self {
        let replies: Vec<Value> = serde_json::from_str(fixture).expect("fixture is a list");
        Self::replying_in_turn(replies)
    }

    fn replying_in_turn(replies: Vec<Value>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().rev().collect()),
            asked: Arc::default(),
        }
    }

    /// What the gateway asked, readable after the transport moved into the gateway.
    fn asked(&self) -> Arc<Mutex<Vec<(String, Value)>>> {
        Arc::clone(&self.asked)
    }
}

impl RpcTransport for RecordedTransport {
    fn call(&self, method: &str, params: Value, _timeout: Duration) -> Result<Value, GatewayError> {
        self.asked
            .lock()
            .expect("lock")
            .push((method.to_owned(), params));
        self.replies
            .lock()
            .expect("lock")
            .pop()
            .ok_or_else(|| GatewayError::Unavailable("no recorded reply left".to_owned()))
    }
}

const PROVIDERS: &str = include_str!("fixtures/sidecar_replies/load_extension.json");
const SEARCH: &str = include_str!("fixtures/sidecar_replies/search.json");
const MAIN_PAGE: &str = include_str!("fixtures/sidecar_replies/main_page.json");
const MOVIE: &str = include_str!("fixtures/sidecar_replies/load_movie.json");
const SERIES: &str = include_str!("fixtures/sidecar_replies/load_series.json");
const POLLS: &str = include_str!("fixtures/sidecar_replies/poll_link_search.json");

fn gateway(transport: RecordedTransport) -> RpcProviderGateway<RecordedTransport> {
    RpcProviderGateway::new(transport)
}

#[test]
fn the_provider_list_of_an_extension_is_decoded() {
    let gateway = gateway(RecordedTransport::replying_once(PROVIDERS));
    let providers = gateway
        .load_extension(Path::new("CineStream.cs3"), false)
        .expect("the recorded reply decodes");
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].name, "CineStream");
    assert_eq!(providers[0].main_page.len(), 3);
    assert_eq!(providers[0].main_page[0].name, "Top Movies");
}

#[test]
fn a_search_page_is_decoded_with_its_titles_and_kinds() {
    let gateway = gateway(RecordedTransport::replying_once(SEARCH));
    let page = gateway
        .search("CineStream", "Spider-Man", 1)
        .expect("the recorded reply decodes");
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.items[0].name, "Spider-Man: Brand New Day");
    assert_eq!(page.items[0].api_name, "CineStream");
    assert!(!page.has_next);
}

#[test]
fn a_home_page_section_is_decoded_with_its_titles() {
    let gateway = gateway(RecordedTransport::replying_once(MAIN_PAGE));
    let entry = MainPageEntry {
        name: "Top Movies".to_owned(),
        data: "https://host.example.test/top".to_owned(),
        horizontal_images: false,
    };
    let page = gateway
        .main_page("CineStream", 1, &entry)
        .expect("the recorded reply decodes")
        .expect("a page");
    assert_eq!(page.items[0].name, "Top Movies");
    assert_eq!(page.items[0].list.len(), 2);
    assert_eq!(page.items[0].list[0].name, "Obsession");
}

#[test]
fn a_movie_is_decoded_with_its_stream_payload_rating_background_and_cast() {
    let gateway = gateway(RecordedTransport::replying_once(MOVIE));
    let movie = gateway
        .load("CineStream", "{\"id\":\"tt22084616\",\"type\":\"movie\"}")
        .expect("the recorded reply decodes")
        .expect("details");
    assert_eq!(movie.name, "Spider-Man: Brand New Day");
    assert_eq!(movie.media_type.as_deref(), Some("Movie"));
    assert!(movie
        .data_url
        .is_some_and(|data| data.contains("tt22084616")));
    assert_eq!(movie.year, Some(2026));
    assert_eq!(movie.rating, Some(8000));
    assert!(movie.background_poster_url.is_some());
    assert!(movie.episodes.is_empty());
    assert_eq!(movie.actors.len(), 1);
    assert!(!movie.actors[0].name.is_empty());
}

/// The reply that once failed with "invalid type: map, expected a sequence": a series whose
/// episodes arrive grouped by language.
#[test]
fn a_series_with_episodes_grouped_by_language_is_decoded_into_one_list() {
    let gateway = gateway(RecordedTransport::replying_once(SERIES));
    let series = gateway
        .load("CineStream", "{\"id\":\"tt18546730\",\"type\":\"series\"}")
        .expect("the recorded reply decodes")
        .expect("details");
    assert_eq!(series.name, "The Walking Dead: Dead City");
    assert_eq!(series.episodes.len(), 3);
    let numbers: Vec<(Option<i32>, Option<i32>)> = series
        .episodes
        .iter()
        .map(|episode| (episode.season, episode.episode))
        .collect();
    assert_eq!(
        numbers,
        [(Some(1), Some(1)), (Some(1), Some(2)), (Some(1), Some(3))]
    );
    assert!(series.episodes[0].data.contains("tt18546730"));
    assert!(series.episodes[0]
        .description
        .as_deref()
        .is_some_and(|text| text.contains("Maggie")));
    assert_eq!(
        series.tags,
        Some(vec!["Action".into(), "Adventure".into(), "Drama".into()])
    );
    assert_eq!(series.actors.len(), 2);
}

#[test]
fn successive_link_search_updates_chain_their_cursors_and_carry_links_and_subtitles() {
    let gateway = gateway(RecordedTransport::replying_each(POLLS));
    let mut cursor = LinkSearchCursor::default();
    let (mut links, mut subtitles) = (0, 0);
    for _ in 0..5 {
        let update = gateway
            .poll_link_search("search-id", cursor, Duration::from_millis(1))
            .expect("the recorded update decodes");
        links += update.links.len();
        subtitles += update.subtitles.len();
        assert_eq!(
            update.cursor(),
            LinkSearchCursor {
                links: cursor.links + update.links.len(),
                subtitles: cursor.subtitles + update.subtitles.len()
            },
            "the next cursor must continue where this update ended"
        );
        cursor = update.cursor();
        assert!(!update.done, "the recorded search was still running");
    }
    assert_eq!(links, 11);
    assert_eq!(subtitles, 1);
}

#[test]
fn a_recorded_stream_keeps_its_kind_quality_referer_and_headers() {
    let transport = RecordedTransport::replying_each(POLLS);
    let asked = transport.asked();
    let gateway = gateway(transport);
    let first = gateway
        .poll_link_search("search-id", LinkSearchCursor::default(), Duration::ZERO)
        .expect("the recorded update decodes");
    let link = &first.links[0];
    assert_eq!(link.link_type, "M3U8");
    assert_eq!(link.quality, 1080);
    assert!(link.url.ends_with(".m3u8"));
    assert!(!link.referer.is_empty());
    assert!(link.headers.is_empty());
    let calls = asked.lock().expect("lock");
    assert_eq!(calls[0].0, "poll_link_search");
    assert_eq!(calls[0].1["id"], "search-id");
    assert_eq!(calls[0].1["fromLink"], 0);
}

#[test]
fn a_recorded_subtitle_keeps_its_language_and_address() {
    let gateway = gateway(RecordedTransport::replying_each(POLLS));
    let mut found = None;
    for _ in 0..5 {
        let update = gateway
            .poll_link_search("search-id", LinkSearchCursor::default(), Duration::ZERO)
            .expect("the recorded update decodes");
        found = found.or_else(|| update.subtitles.into_iter().next());
    }
    let subtitle = found.expect("one of the recorded updates carried a subtitle");
    assert_eq!(subtitle.lang, "English");
    assert!(subtitle.url.ends_with(".srt"));
}
