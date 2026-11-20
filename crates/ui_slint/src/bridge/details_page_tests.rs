use super::*;

#[test]
fn resume_label_formats_hours_minutes_and_movies() {
    let hours = WatchProgress {
        season: Some(3),
        episode: Some(1),
        position_ms: 8 * 60_000,
        duration_ms: 70 * 60_000,
        ..WatchProgress::default()
    };
    assert_eq!(resume_label(&hours), "Resume S3 E1 (1h 2m left)");
    let mins = WatchProgress {
        season: Some(1),
        episode: Some(3),
        position_ms: 10 * 60_000,
        duration_ms: 33 * 60_000,
        ..WatchProgress::default()
    };
    assert_eq!(resume_label(&mins), "Resume S1 E3 (23m left)");
    let movie = WatchProgress::default();
    assert_eq!(resume_label(&movie), "Resume");
}

#[test]
fn episode_mark_partial_watched_and_unknown() {
    assert_eq!(episode_mark(0, 1_000), (0.0, false));
    assert_eq!(episode_mark(500, 0), (0.0, false));
    let (half, watched) = episode_mark(500, 1_000);
    assert!((half - 0.5).abs() < 1e-6 && !watched);
    assert!(episode_mark(800, 1_000).1, "80 % counts as watched");
    assert!((episode_mark(2_000, 1_000).0 - 1.0).abs() < 1e-6);
}

#[test]
fn format_details_meta_extracts_badges_and_facts() {
    let page = DetailsPage {
        title: "Severance".into(),
        plot: String::new(),
        resume: String::new(),
        resume_media_key: None,
        episodes: Vec::new(),
        poster_url: None,
        background_url: None,
        provider: "CineStream".into(),
        url: String::new(),
        data: String::new(),
        media_key: String::new(),
        skip_ids: skip_times::ExternalIds::default(),
        media_type: "TvSeries".into(),
        year: Some(2022),
        rating: Some(6800),
        duration_minutes: Some(52),
        tags: Vec::new(),
        cast_members: Vec::new(),
    };
    let meta = format_details_meta(&page);
    assert_eq!(meta.media_type.as_str(), "TVSERIES");
    assert_eq!(meta.provider.as_str(), "CineStream");
    assert_eq!(meta.year.as_str(), "2022");
    assert_eq!(meta.rating.as_str(), "6.8");
    assert_eq!(meta.duration.as_str(), "52 min");
}

#[test]
fn clean_details_text_normalizes_math_bold_plot_and_episodes() {
    let details = LoadDetails {
        name: "𝗦𝗽𝗶𝗱𝗲𝗿-𝗠𝗮𝗻".into(),
        plot: Some("[𝟭 𝘄𝗶𝗻 & 𝟳 𝗻𝗼𝗺𝗶𝗻𝗮𝘁𝗶𝗼𝗻𝘀 𝘁𝗼𝘁𝗮𝗹] New York City.".into()),
        tags: Some(vec!["𝗔𝗰𝘁𝗶𝗼𝗻".into()]),
        episodes: vec![core_model::EpisodeInfo {
            name: Some("𝗣𝗶𝗹𝗼𝘁".into()),
            description: Some("𝗘𝗽𝗶𝘀𝗼𝗱𝗲 𝟭 synopsis".into()),
            data: "ep-1".into(),
            ..core_model::EpisodeInfo::default()
        }],
        ..LoadDetails::default()
    };
    let (title, plot, tags) = clean_details_text(&details, "Fallback");
    assert_eq!(title, "Spider-Man");
    assert_eq!(plot, "[1 win & 7 nominations total] New York City.");
    assert_eq!(tags, ["Action"]);
    let rows = episode_rows(&details, &[]);
    assert_eq!(rows[0].name, "Pilot");
    assert_eq!(rows[0].description, "Episode 1 synopsis");
}
