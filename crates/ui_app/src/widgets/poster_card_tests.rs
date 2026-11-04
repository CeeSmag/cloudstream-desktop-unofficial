use super::*;

#[test]
fn badge_label_recognises_common_media_types() {
    assert_eq!(badge_label("Movie"), Some("MOVIE"));
    assert_eq!(badge_label("TvSeries"), Some("SERIES"));
    assert_eq!(badge_label("Anime"), Some("ANIME"));
    assert_eq!(badge_label("Unknown"), None);
}

#[test]
fn parse_rating_extracts_valid_scores() {
    assert_eq!(parse_rating("Movie \u{00B7} 8.5"), Some("8.5"));
    assert_eq!(parse_rating("\u{2605} 9.2"), Some("9.2"));
    assert_eq!(parse_rating("TV Series"), None);
}
