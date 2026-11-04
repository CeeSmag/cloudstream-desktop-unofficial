use super::*;

#[test]
fn play_request_roundtrip_preserves_fields() {
    let req = PlayRequestDto {
        provider: "TestProvider".to_owned(),
        media_key: "ep-101".to_owned(),
        title: "Episode 1".to_owned(),
        detail_url: Some("https://example.com/show".to_owned()),
        poster_url: Some("https://example.com/img.jpg".to_owned()),
        season: Some(1),
        episode: Some(2),
        next_episode: Some(NextEpisodeDto {
            media_key: "ep-102".to_owned(),
            season: Some(1),
            episode: Some(3),
        }),
    };
    let json = serde_json::to_string(&req).expect("serialize");
    let parsed: PlayRequestDto = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.provider, "TestProvider");
    assert_eq!(parsed.media_key, "ep-101");
    assert_eq!(parsed.title, "Episode 1");
}

#[test]
fn playback_status_default_is_empty() {
    let status = PlaybackStatusDto::default();
    assert_eq!(status.position_ms, 0);
    assert_eq!(status.duration_ms, 0);
    assert!(!status.has_video);
    assert!(status.tracks.is_empty());
}

#[test]
fn home_section_roundtrip() {
    let item = HomeItemDto {
        id: "1".to_owned(),
        name: "Movie".to_owned(),
        url: "https://example.com/1".to_owned(),
        poster_url: None,
        provider: "Provider".to_owned(),
        media_type: Some("Movie".to_owned()),
        year: Some(2025),
    };
    let section = HomeSectionDto {
        title: "Trending".to_owned(),
        items: vec![item],
        is_horizontal: false,
    };
    let json = serde_json::to_string(&section).expect("serialize");
    let parsed: HomeSectionDto = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.title, "Trending");
    assert_eq!(parsed.items.len(), 1);
    assert_eq!(parsed.items[0].name, "Movie");
}
