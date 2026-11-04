//! Asks the real databases. Needs the internet, so it only runs on request:
//! `cargo test -p skip_times --test live_lookup -- --ignored`

use std::sync::Arc;

use skip_times::{ExternalIds, SkipKind, SkipLookup, SkipQuery, TitleKind, UreqSkipFetcher};

fn lookup() -> SkipLookup {
    SkipLookup::new(Arc::new(UreqSkipFetcher::default()))
}

#[test]
#[ignore = "needs the internet"]
fn a_known_episode_of_a_series_has_an_intro() {
    let query = SkipQuery {
        ids: ExternalIds::new(Some("tt18546730"), None, None),
        kind: TitleKind::Series,
        season: Some(1),
        episode: Some(1),
        duration_ms: 3_500_000,
    };
    let found = lookup().find(&query).expect("the databases answered");
    assert!(found.iter().any(|s| s.kind == SkipKind::Intro), "{found:?}");
}

#[test]
#[ignore = "needs the internet"]
fn a_known_anime_episode_has_an_opening_and_an_ending() {
    let query = SkipQuery {
        ids: ExternalIds::new(None, None, Some("40748")),
        kind: TitleKind::Anime,
        season: Some(1),
        episode: Some(1),
        duration_ms: 1_440_000,
    };
    let found = lookup().find(&query).expect("the databases answered");
    assert!(found.iter().any(|s| s.kind == SkipKind::Intro), "{found:?}");
    assert!(
        found.iter().any(|s| s.kind == SkipKind::Credits),
        "{found:?}"
    );
}

#[test]
#[ignore = "needs the internet"]
fn an_unknown_episode_is_an_empty_answer_not_an_error() {
    let query = SkipQuery {
        ids: ExternalIds::new(Some("tt0000001"), None, None),
        kind: TitleKind::Series,
        season: Some(1),
        episode: Some(1),
        duration_ms: 1_000_000,
    };
    assert!(lookup().find(&query).expect("answered").is_empty());
}
