use super::*;
use crate::external_ids::ExternalIds;
use crate::test_fetcher::ScriptedFetcher;

fn query(kind: TitleKind, ids: ExternalIds) -> SkipQuery {
    SkipQuery {
        ids,
        kind,
        season: Some(1),
        episode: Some(2),
        duration_ms: 3_000_000,
    }
}

fn imdb() -> ExternalIds {
    ExternalIds::new(Some("tt18546730"), None, None)
}

/// A real answer of the service (series, S1E1 of tt18546730).
const REAL_EPISODE: &str = r#"{"tvdb_id":417549,"type":"tv","season":1,"episode":1,"intro":[{"start_ms":162999,"end_ms":210000}]}"#;
/// A real answer for a film: the intro has no start and the credits no end.
const REAL_MOVIE: &str = r#"{"tvdb_id":169,"type":"movie","intro":[{"start_ms":null,"end_ms":40000}],"credits":[{"start_ms":7726802,"end_ms":null}]}"#;

#[test]
fn an_episode_is_asked_by_imdb_id_season_and_episode() {
    let fetcher = ScriptedFetcher::answering(REAL_EPISODE);
    let found = segments(&fetcher, &query(TitleKind::Series, imdb())).expect("found");
    assert_eq!(
        fetcher.urls(),
        ["https://api.theintrodb.org/v4/media?imdb_id=tt18546730&season=1&episode=2"]
    );
    assert_eq!(
        found,
        [SkipSegment {
            kind: SkipKind::Intro,
            start_ms: 162_999,
            end_ms: 210_000
        }]
    );
}

#[test]
fn tmdb_is_preferred_and_a_film_is_asked_without_season_and_episode() {
    let fetcher = ScriptedFetcher::answering(REAL_MOVIE);
    let ids = ExternalIds::new(Some("tt0133093"), Some("603"), None);
    let found = segments(&fetcher, &query(TitleKind::Movie, ids)).expect("found");
    assert_eq!(
        fetcher.urls(),
        ["https://api.theintrodb.org/v4/media?tmdb_id=603"]
    );
    assert_eq!(found.len(), 2);
}

#[test]
fn a_missing_start_is_the_beginning_and_a_missing_end_is_the_end_of_the_video() {
    let fetcher = ScriptedFetcher::answering(REAL_MOVIE);
    let found = segments(&fetcher, &query(TitleKind::Movie, imdb())).expect("found");
    assert_eq!(found[0].start_ms, 0);
    assert_eq!(found[0].end_ms, 40_000);
    assert_eq!(found[1].kind, SkipKind::Credits);
    assert_eq!(found[1].start_ms, 7_726_802);
    assert_eq!(found[1].end_ms, 3_000_000);
}

#[test]
fn nothing_is_asked_without_an_id_or_without_the_episode_of_a_series() {
    let fetcher = ScriptedFetcher::answering(REAL_EPISODE);
    let none = query(TitleKind::Series, ExternalIds::default());
    assert!(segments(&fetcher, &none).expect("ok").is_empty());
    let mut no_episode = query(TitleKind::Series, imdb());
    no_episode.episode = None;
    assert!(segments(&fetcher, &no_episode).expect("ok").is_empty());
    assert!(fetcher.urls().is_empty());
}

#[test]
fn not_found_is_an_empty_answer_but_other_failures_are_errors() {
    let missing = ScriptedFetcher::failing(SkipError::Status { status: 404 });
    assert!(segments(&missing, &query(TitleKind::Series, imdb()))
        .expect("ok")
        .is_empty());
    let down = ScriptedFetcher::failing(SkipError::Status { status: 503 });
    assert_eq!(
        segments(&down, &query(TitleKind::Series, imdb())),
        Err(SkipError::Status { status: 503 })
    );
    let garbage = ScriptedFetcher::answering("<html>");
    assert!(matches!(
        segments(&garbage, &query(TitleKind::Series, imdb())),
        Err(SkipError::Invalid { .. })
    ));
}
