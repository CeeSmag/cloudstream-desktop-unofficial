use super::*;
use core_model::EpisodeInfo;

#[test]
fn strip_markup_removes_tags_and_decodes_basic_entities() {
    assert_eq!(strip_markup("A<br>\n<br>B &amp; C"), "A B & C");
}

#[test]
fn meta_line_skips_missing_values() {
    assert_eq!(
        meta_line(&LoadDetails {
            api_name: "Archive".into(),
            year: Some(2019),
            ..LoadDetails::default()
        }),
        "Archive \u{00B7} 2019"
    );
}

#[test]
fn playable_data_prefers_movie_data_then_first_episode() {
    let movie = LoadDetails {
        name: "M".into(),
        data_url: Some("id".into()),
        ..LoadDetails::default()
    };
    assert_eq!(
        playable_data(&movie),
        Some(("id".to_owned(), "M".to_owned()))
    );
    let series = LoadDetails {
        name: "S".into(),
        episodes: vec![EpisodeInfo {
            data: "ep".into(),
            episode: Some(1),
            ..EpisodeInfo::default()
        }],
        ..LoadDetails::default()
    };
    assert_eq!(
        playable_data(&series).map(|found| found.0),
        Some("ep".to_owned())
    );
    assert_eq!(playable_data(&LoadDetails::default()), None);
}
