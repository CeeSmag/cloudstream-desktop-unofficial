use super::*;
use crate::external_ids::ExternalIds;
use crate::skip_segment::SkipKind;
use crate::test_fetcher::ScriptedFetcher;

const AN_INTRO: &str = r#"{"intro":[{"start_ms":10000,"end_ms":60000}]}"#;
const NOTHING_ANI: &str = r#"{"found":false,"results":[]}"#;
const ANI_OPENING: &str =
    r#"{"found":true,"results":[{"interval":{"startTime":5,"endTime":95},"skipType":"op"}]}"#;
const INTRO_DB_OUTRO: &str =
    r#"{"intro":null,"recap":null,"outro":{"start_ms":900000,"end_ms":1000000}}"#;

fn query(kind: TitleKind) -> SkipQuery {
    SkipQuery {
        ids: ExternalIds::new(Some("tt1234567"), Some("603"), Some("21")),
        kind,
        season: Some(1),
        episode: Some(3),
        duration_ms: 1_000_000,
    }
}

fn lookup(fetcher: &Arc<ScriptedFetcher>) -> SkipLookup {
    SkipLookup::new(Arc::clone(fetcher) as Arc<dyn SkipFetcher>)
}

#[test]
fn anime_asks_aniskip_first_and_stops_at_the_first_database_that_knows() {
    let fetcher = Arc::new(
        ScriptedFetcher::answering(AN_INTRO)
            .route("https://api.aniskip", Ok(ANI_OPENING.to_owned())),
    );
    let found = lookup(&fetcher)
        .find(&query(TitleKind::Anime))
        .expect("found");
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].start_ms, found[0].end_ms), (5000, 95_000));
    assert_eq!(fetcher.calls(), 1, "TheIntroDB was never asked");
}

#[test]
fn an_empty_answer_moves_on_to_the_next_database() {
    let fetcher = Arc::new(
        ScriptedFetcher::answering(AN_INTRO)
            .route("https://api.aniskip", Ok(NOTHING_ANI.to_owned())),
    );
    let found = lookup(&fetcher)
        .find(&query(TitleKind::Anime))
        .expect("found");
    assert_eq!(found[0].kind, SkipKind::Intro);
    assert_eq!(fetcher.calls(), 2);
}

#[test]
fn a_series_falls_back_from_theintrodb_to_introdb() {
    let fetcher = Arc::new(
        ScriptedFetcher::answering(INTRO_DB_OUTRO)
            .route("https://api.theintrodb", Ok("{}".to_owned())),
    );
    let found = lookup(&fetcher)
        .find(&query(TitleKind::Series))
        .expect("found");
    assert_eq!(found[0].kind, SkipKind::Credits);
    assert_eq!(fetcher.calls(), 2);
}

#[test]
fn a_database_that_is_down_is_skipped_and_the_next_one_answers() {
    let fetcher = Arc::new(ScriptedFetcher::answering(INTRO_DB_OUTRO).route(
        "https://api.theintrodb",
        Err(SkipError::Status { status: 500 }),
    ));
    assert_eq!(
        lookup(&fetcher)
            .find(&query(TitleKind::Series))
            .expect("found")
            .len(),
        1
    );
}

#[test]
fn when_every_database_is_down_it_is_an_error_and_nothing_is_remembered() {
    let fetcher = Arc::new(ScriptedFetcher::failing(SkipError::Network {
        reason: "offline".to_owned(),
    }));
    let lookup = lookup(&fetcher);
    assert!(lookup.find(&query(TitleKind::Series)).is_err());
    assert_eq!(lookup.remembered(), 0);
    assert!(lookup.find(&query(TitleKind::Series)).is_err());
    assert_eq!(fetcher.calls(), 4, "asked again, both databases each time");
}

#[test]
fn an_answer_of_nothing_is_remembered_so_the_episode_is_asked_about_once() {
    let fetcher = Arc::new(ScriptedFetcher::answering("{}"));
    let lookup = lookup(&fetcher);
    assert!(lookup
        .find(&query(TitleKind::Movie))
        .expect("ok")
        .is_empty());
    assert!(lookup
        .find(&query(TitleKind::Movie))
        .expect("ok")
        .is_empty());
    assert_eq!(fetcher.calls(), 1);
    assert_eq!(lookup.remembered(), 1);
}

#[test]
fn another_episode_or_another_title_is_a_new_question() {
    let fetcher = Arc::new(ScriptedFetcher::answering(AN_INTRO));
    let lookup = lookup(&fetcher);
    let mut other_episode = query(TitleKind::Series);
    lookup.find(&other_episode).expect("ok");
    other_episode.episode = Some(4);
    lookup.find(&other_episode).expect("ok");
    assert_eq!(fetcher.calls(), 2);
}

#[test]
fn a_title_without_usable_ids_is_not_asked_at_all() {
    let fetcher = Arc::new(ScriptedFetcher::answering(AN_INTRO));
    let mut q = query(TitleKind::Series);
    q.ids = ExternalIds::default();
    assert!(lookup(&fetcher).find(&q).expect("ok").is_empty());
    assert_eq!(fetcher.calls(), 0);
}

#[test]
fn answers_are_cut_to_the_video_length() {
    let fetcher = Arc::new(ScriptedFetcher::answering(
        r#"{"credits":[{"start_ms":900000,"end_ms":5000000}]}"#,
    ));
    let found = lookup(&fetcher)
        .find(&query(TitleKind::Movie))
        .expect("found");
    assert_eq!(found[0].end_ms, 1_000_000);
}
