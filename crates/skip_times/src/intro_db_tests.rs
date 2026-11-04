use super::*;
use crate::external_ids::ExternalIds;
use crate::test_fetcher::ScriptedFetcher;

fn query(kind: TitleKind) -> SkipQuery {
    SkipQuery {
        ids: ExternalIds::new(Some("tt18546730"), None, None),
        kind,
        season: Some(1),
        episode: Some(1),
        duration_ms: 0,
    }
}

/// A real answer of the service.
const REAL: &str = r#"{"imdb_id":"tt18546730","media_type":"tv","is_movie":false,"season":1,"episode":1,"intro":{"start_sec":166,"end_sec":216,"start_ms":166000,"end_ms":216000,"confidence":1,"submission_count":1,"updated_at":"2026-04-08T19:13:42.532Z"},"recap":null,"outro":{"start_sec":3269,"end_sec":3301,"start_ms":3269000,"end_ms":3301000,"confidence":1},"post_credits":null}"#;
/// A real answer when nothing is known.
const NOTHING: &str =
    r#"{"imdb_id":"tt0000001","intro":null,"recap":null,"outro":null,"post_credits":null}"#;

fn segment(kind: SkipKind, start_ms: u64, end_ms: u64) -> SkipSegment {
    SkipSegment {
        kind,
        start_ms,
        end_ms,
    }
}

#[test]
fn intro_and_outro_are_read_and_a_null_recap_is_left_out() {
    let fetcher = ScriptedFetcher::answering(REAL);
    let found = segments(&fetcher, &query(TitleKind::Series)).expect("found");
    assert_eq!(
        fetcher.urls(),
        ["https://api.introdb.app/segments?imdb_id=tt18546730&season=1&episode=1"]
    );
    assert_eq!(
        found,
        [
            segment(SkipKind::Intro, 166_000, 216_000),
            segment(SkipKind::Credits, 3_269_000, 3_301_000)
        ]
    );
}

#[test]
fn an_answer_with_nothing_known_is_empty() {
    let fetcher = ScriptedFetcher::answering(NOTHING);
    assert!(segments(&fetcher, &query(TitleKind::Series))
        .expect("ok")
        .is_empty());
}

#[test]
fn films_and_titles_without_an_imdb_id_are_not_asked() {
    let fetcher = ScriptedFetcher::answering(REAL);
    assert!(segments(&fetcher, &query(TitleKind::Movie))
        .expect("ok")
        .is_empty());
    let mut no_id = query(TitleKind::Series);
    no_id.ids = ExternalIds::default();
    assert!(segments(&fetcher, &no_id).expect("ok").is_empty());
    assert!(fetcher.urls().is_empty());
}

#[test]
fn a_segment_missing_one_end_is_dropped_and_failures_are_reported() {
    let half = ScriptedFetcher::answering(r#"{"intro":{"start_ms":5000,"end_ms":null}}"#);
    assert!(segments(&half, &query(TitleKind::Series))
        .expect("ok")
        .is_empty());
    let down = ScriptedFetcher::failing(SkipError::Network {
        reason: "offline".to_owned(),
    });
    assert!(segments(&down, &query(TitleKind::Series)).is_err());
}
