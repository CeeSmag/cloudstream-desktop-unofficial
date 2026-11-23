use super::*;

fn labels(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn every_library_name_reads_back_to_its_type() {
    for (name, media) in CANONICAL {
        assert_eq!(MediaType::from_label(name), Some(*media), "{name}");
    }
    // As the library writes them, and with stray case and blanks.
    assert_eq!(MediaType::from_label("TvSeries"), Some(MediaType::TvSeries));
    assert_eq!(
        MediaType::from_label(" AsianDrama "),
        Some(MediaType::AsianDrama)
    );
    assert_eq!(MediaType::from_label("NSFW"), Some(MediaType::Nsfw));
    assert_eq!(MediaType::from_label("OVA"), Some(MediaType::Ova));
}

#[test]
fn every_type_name_reads_back_to_the_same_type() {
    for (_, media) in CANONICAL {
        assert_eq!(MediaType::from_label(media.name()), Some(*media));
    }
}

#[test]
fn spellings_seen_in_repositories_are_normalised() {
    let cases = [
        ("Movies", MediaType::Movie),
        ("Series", MediaType::TvSeries),
        ("TV Series", MediaType::TvSeries),
        ("Drama", MediaType::TvSeries),
        ("live", MediaType::Live),
        ("TvChannel", MediaType::Live),
        ("Cartoons", MediaType::Cartoon),
        ("AnimeMovies", MediaType::AnimeMovie),
        ("ONA", MediaType::Ova),
        ("Special", MediaType::Ova),
        ("Other", MediaType::Others),
    ];
    for (label, media) in cases {
        assert_eq!(MediaType::from_label(label), Some(media), "{label}");
    }
}

#[test]
fn unreadable_labels_are_not_guessed() {
    for label in ["All", "tvTypes", "", "Movi", "Games", "Anything"] {
        assert_eq!(MediaType::from_label(label), None, "{label}");
    }
}

#[test]
fn aliases_never_shadow_a_library_name() {
    for (alias, _) in ALIASES {
        assert!(
            CANONICAL.iter().all(|(name, _)| name != alias),
            "alias {alias} duplicates a library name"
        );
        assert_eq!(
            *alias,
            alias.to_lowercase(),
            "aliases are stored lower case"
        );
    }
}

#[test]
fn video_is_everything_but_audio_books_and_catch_alls() {
    let video = [
        MediaType::Movie,
        MediaType::AnimeMovie,
        MediaType::TvSeries,
        MediaType::Cartoon,
        MediaType::Anime,
        MediaType::Ova,
        MediaType::Torrent,
        MediaType::Documentary,
        MediaType::AsianDrama,
        MediaType::Live,
        MediaType::Nsfw,
        MediaType::Video,
    ];
    let not_video = [
        MediaType::Others,
        MediaType::Music,
        MediaType::AudioBook,
        MediaType::CustomMedia,
        MediaType::Audio,
        MediaType::Podcast,
    ];
    assert!(video.iter().all(|m| m.is_video()));
    assert!(not_video.iter().all(|m| !m.is_video()));
    assert_eq!(
        video.len() + not_video.len(),
        CANONICAL.len(),
        "every type is classified"
    );
}

#[test]
fn comma_lists_and_duplicates_are_read_once_each() {
    let parsed = parse_labels(&labels(&["Movie,Anime,Cartoon", "Movie", " ", "tvTypes"]));
    assert_eq!(
        parsed.types,
        vec![MediaType::Movie, MediaType::Cartoon, MediaType::Anime]
    );
    assert_eq!(parsed.unrecognised, vec!["tvTypes".to_owned()]);
}

#[test]
fn no_labels_parse_to_nothing() {
    assert_eq!(parse_labels(&[]), ParsedLabels::default());
}
