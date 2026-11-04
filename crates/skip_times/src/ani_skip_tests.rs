use super::*;
use crate::external_ids::ExternalIds;
use crate::test_fetcher::ScriptedFetcher;

fn query(kind: TitleKind) -> SkipQuery {
    SkipQuery {
        ids: ExternalIds::new(None, None, Some("40748")),
        kind,
        season: Some(1),
        episode: Some(1),
        duration_ms: 1_440_000,
    }
}

/// A real answer of the service (episode 1 of MAL 40748).
const REAL: &str = r#"{"found":true,"results":[{"interval":{"startTime":1338.255,"endTime":1428.255},"skipType":"ed","skipId":"a","episodeLength":1435.028},{"interval":{"startTime":54.711,"endTime":145.111},"skipType":"op","skipId":"b","episodeLength":1435.122}],"message":"Successfully found skip times","statusCode":200}"#;
/// A real answer when nothing is known.
const NOTHING: &str =
    r#"{"found":false,"results":[],"message":"No skip times found","statusCode":404}"#;

#[test]
fn opening_and_ending_are_read_in_milliseconds() {
    let fetcher = ScriptedFetcher::answering(REAL);
    let found = segments(&fetcher, &query(TitleKind::Anime)).expect("found");
    assert_eq!(
        found,
        [
            SkipSegment {
                kind: SkipKind::Credits,
                start_ms: 1_338_255,
                end_ms: 1_428_255
            },
            SkipSegment {
                kind: SkipKind::Intro,
                start_ms: 54_711,
                end_ms: 145_111
            }
        ]
    );
}

#[test]
fn the_request_names_the_mal_id_the_episode_and_the_length_in_seconds() {
    let fetcher = ScriptedFetcher::answering(NOTHING);
    segments(&fetcher, &query(TitleKind::Anime)).expect("ok");
    let urls = fetcher.urls();
    assert!(urls[0].starts_with("https://api.aniskip.com/v2/skip-times/40748/1?types%5B%5D=ed"));
    assert!(urls[0].ends_with("&episodeLength=1440"), "{}", urls[0]);
}

#[test]
fn only_anime_with_a_mal_id_are_asked() {
    let fetcher = ScriptedFetcher::answering(REAL);
    assert!(segments(&fetcher, &query(TitleKind::Series))
        .expect("ok")
        .is_empty());
    let mut no_id = query(TitleKind::Anime);
    no_id.ids = ExternalIds::default();
    assert!(segments(&fetcher, &no_id).expect("ok").is_empty());
    assert!(fetcher.urls().is_empty());
}

#[test]
fn unknown_types_are_ignored_and_not_found_is_empty() {
    let preview = ScriptedFetcher::answering(
        r#"{"found":true,"results":[{"interval":{"startTime":1,"endTime":2},"skipType":"mixed-op"},{"interval":{"startTime":1,"endTime":2},"skipType":"weird"}]}"#,
    );
    let found = segments(&preview, &query(TitleKind::Anime)).expect("ok");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, SkipKind::Intro);
    let nothing = ScriptedFetcher::answering(NOTHING);
    assert!(segments(&nothing, &query(TitleKind::Anime))
        .expect("ok")
        .is_empty());
    let missing = ScriptedFetcher::failing(SkipError::Status { status: 404 });
    assert!(segments(&missing, &query(TitleKind::Anime))
        .expect("ok")
        .is_empty());
}
