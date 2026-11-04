use super::*;

fn sync(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn cinestream_sends_the_ids_as_json_text_under_an_empty_key() {
    let ids = ExternalIds::from_sync(&sync(&[("", r#"{"Imdb":"tt18546730"}"#)]), None);
    assert_eq!(ids.imdb.as_deref(), Some("tt18546730"));
    assert_eq!(ids.tmdb, None);
}

#[test]
fn json_text_may_carry_several_ids_as_text_or_numbers() {
    let ids = ExternalIds::from_sync(
        &sync(&[("simkl", r#"{"imdb":"tt0903747","tmdb":1396,"Mal":"21"}"#)]),
        None,
    );
    assert_eq!(ids.imdb.as_deref(), Some("tt0903747"));
    assert_eq!(ids.tmdb.as_deref(), Some("1396"));
    assert_eq!(ids.mal.as_deref(), Some("21"));
}

#[test]
fn plain_ids_are_read_by_the_name_of_their_key() {
    let ids = ExternalIds::from_sync(&sync(&[("MalSync", "40748"), ("imdb", "tt1234567")]), None);
    assert_eq!(ids.mal.as_deref(), Some("40748"));
    assert_eq!(ids.imdb.as_deref(), Some("tt1234567"));
}

#[test]
fn the_unique_url_fills_in_a_missing_imdb_id_but_does_not_replace_one() {
    let unique = Some(r#"{"id":"tt22084616","type":"movie"}"#);
    let from_url_only = ExternalIds::from_sync(&BTreeMap::new(), unique);
    assert_eq!(from_url_only.imdb.as_deref(), Some("tt22084616"));
    let ids = ExternalIds::from_sync(&sync(&[("imdb", "tt1111111")]), unique);
    assert_eq!(ids.imdb.as_deref(), Some("tt1111111"));
}

#[test]
fn malformed_ids_are_dropped_so_they_cannot_reach_a_url() {
    let ids = ExternalIds::new(Some("tt1&evil=1"), Some("12/../x"), Some(""));
    assert!(ids.is_empty());
    let ids = ExternalIds::new(Some("tt123"), Some("12345678901"), Some("-4"));
    assert!(ids.is_empty(), "too short, too long, not a number");
    assert!(!ExternalIds::new(Some(" tt1234567 "), None, None).is_empty());
}

#[test]
fn garbage_gives_no_ids_and_no_panic() {
    let ids = ExternalIds::from_sync(
        &sync(&[("imdb", "{broken"), ("x", "[1,2]"), ("", "null")]),
        Some("not json"),
    );
    assert!(ids.is_empty());
}
