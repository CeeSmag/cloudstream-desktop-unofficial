use app_services::{LinkHealth, PreferredResolution};
use core_model::{PlayableLink, SubtitleTrack};
use player_api::find_language;

use super::*;

fn link(name: &str, quality: i32) -> PlayableLink {
    PlayableLink {
        name: name.to_owned(),
        url: format!("https://cdn.example.com/{name}.mkv"),
        quality,
        ..PlayableLink::default()
    }
}

fn working(size: Option<u64>, subtitles: &[&str]) -> Option<LinkHealth> {
    Some(LinkHealth::Working {
        size_bytes: size,
        subtitles: subtitles.iter().map(|code| (*code).to_owned()).collect(),
    })
}

fn languages(names: &[&str]) -> Vec<player_api::Language> {
    names
        .iter()
        .filter_map(|name| find_language(name))
        .collect()
}

fn indexes(menu: &SourceMenu) -> Vec<usize> {
    menu.rows.iter().map(|row| row.index).collect()
}

#[test]
fn source_menu_hides_broken_streams_but_keeps_the_playing_one() {
    let links = vec![
        (link("a", 720), working(None, &[])),
        (link("b", 720), Some(LinkHealth::Broken("404".into()))),
        (link("c", 720), None),
        (link("d", 720), Some(LinkHealth::Broken("dead".into()))),
    ];
    let list = SourceList {
        links: &links,
        subtitles: &[],
        selected: 3,
        preferred: &[],
    };
    let menu = source_menu(&list, &SourceFilter::default());
    assert_eq!(indexes(&menu), [0, 2, 3]);
    assert_eq!(menu.total, 3);
    assert!(menu.rows.get(1).is_some_and(|row| row.checking));
    assert!(menu.rows.get(2).is_some_and(|row| row.selected));
}

#[test]
fn source_menu_sorts_by_file_size_by_default_without_resolution_filter() {
    let mut hls = link("hls", 2160);
    hls.link_type = "M3U8".into();
    let links = vec![
        (hls, working(None, &[])),
        (link("small-4k", 2160), working(Some(900_000_000), &[])),
        (link("big-1080", 1080), working(Some(2_800_000_000), &[])),
        (link("mid-720", 720), working(Some(1_400_000_000), &[])),
        (link("unsized-1080", 1080), working(None, &[])),
    ];
    let list = SourceList {
        links: &links,
        subtitles: &[],
        selected: 0,
        preferred: &[],
    };
    let menu = source_menu(&list, &SourceFilter::default());
    assert_eq!(
        indexes(&menu),
        [2, 3, 1, 0, 4],
        "biggest known files first across all resolutions, then unknown sizes by quality"
    );
    assert_eq!(menu.rows.first().map(|r| r.size.as_str()), Some("2.6 GB"));
    assert_eq!(menu.rows.get(3).map(|r| r.kind.as_str()), Some("HLS"));
    assert_eq!(menu.rows.get(3).map(|r| r.size.as_str()), Some(""));
}

#[test]
fn source_menu_keeps_video_above_larger_audio_only_stream() {
    let mut audio = link("audio", 1080);
    audio.url = "https://cdn.example.com/track.mp3".into();
    let links = vec![
        (audio, working(Some(50_000_000), &[])),
        (
            link("video-unknown-q", 0),
            working(Some(3_000_000_000), &[]),
        ),
        (link("video-1080", 1080), working(Some(1_000_000_000), &[])),
        (link("video-unsized", 720), working(None, &[])),
    ];
    let list = SourceList {
        links: &links,
        subtitles: &[],
        selected: 2,
        preferred: &[],
    };
    let menu = source_menu(&list, &SourceFilter::default());
    assert_eq!(
        indexes(&menu),
        [1, 2, 3, 0],
        "unknown-quality video sorts by file size with other video streams, above audio-only"
    );
}

#[test]
fn source_menu_resolution_filter_counts_groups_and_keeps_biggest_first() {
    let links = vec![
        (link("small", 1080), working(Some(700_000_000), &[])),
        (link("unknown", 0), working(Some(9_000_000_000), &[])),
        (link("big", 720), working(Some(2_000_000_000), &[])),
        (link("uhd", 2160), working(Some(4_000_000_000), &[])),
        (link("nosize", 1080), working(None, &[])),
    ];
    let list = SourceList {
        links: &links,
        subtitles: &[],
        selected: 3,
        preferred: &[],
    };
    let mut filter = SourceFilter::default();
    filter.toggle_tier(Tier::Hd);
    let menu = source_menu(&list, &filter);
    assert_eq!(
        (menu.hd_count, menu.uhd_count, menu.resolution_shown),
        (3, 1, 3)
    );
    assert_eq!(
        indexes(&menu),
        [2, 0, 4],
        "sized biggest first, then unsized"
    );
    assert!(
        !indexes(&menu).contains(&3),
        "the playing source is not kept: it would look like a match"
    );
}

#[test]
fn source_filter_toggle_tier_allows_at_most_one_resolution() {
    let links = vec![
        (link("hd", 1080), working(Some(2_000_000_000), &[])),
        (link("uhd", 2160), working(Some(4_000_000_000), &[])),
    ];
    let list = SourceList {
        links: &links,
        subtitles: &[],
        selected: 0,
        preferred: &[],
    };
    let mut filter = SourceFilter::default();
    filter.toggle_tier(Tier::Hd);
    assert_eq!(filter.tier, Some(Tier::Hd));
    filter.toggle_tier(Tier::Uhd);
    assert_eq!(
        filter.tier,
        Some(Tier::Uhd),
        "replaces Hd instead of keeping both"
    );
    assert_eq!(indexes(&source_menu(&list, &filter)), [1]);
    filter.toggle_tier(Tier::Uhd);
    assert_eq!(filter.tier, None, "second press clears to none");
    assert_eq!(indexes(&source_menu(&list, &filter)), [1, 0]);
}

#[test]
fn source_filter_for_preferred_maps_each_setting_to_initial_tier() {
    assert_eq!(
        SourceFilter::for_preferred(PreferredResolution::Hd).tier,
        Some(Tier::Hd)
    );
    assert_eq!(
        SourceFilter::for_preferred(PreferredResolution::Uhd).tier,
        Some(Tier::Uhd)
    );
    assert_eq!(
        SourceFilter::for_preferred(PreferredResolution::Any).tier,
        None
    );
}

#[test]
fn preferred_resolution_labels_round_trip() {
    let labels = preferred_resolution_labels();
    assert_eq!(labels, vec!["1080p", "2160p", "Any (largest file)"]);
    for choice in PreferredResolution::ALL {
        let label = preferred_resolution_label(choice);
        assert_eq!(parse_preferred_resolution_label(label), Some(choice));
    }
    assert_eq!(parse_preferred_resolution_label("unknown"), None);
}

#[test]
fn source_menu_subtitle_filter_needs_every_wanted_language() {
    let links = vec![
        (link("both", 1080), working(None, &["ara", "eng"])),
        (link("english", 1080), working(None, &["English"])),
        (link("none", 1080), working(None, &[])),
    ];
    let preferred = languages(&["English", "Arabic"]);
    let list = SourceList {
        links: &links,
        subtitles: &[],
        selected: 2,
        preferred: &preferred,
    };
    let mut filter = SourceFilter::default();
    filter.toggle_subtitle("English");
    filter.toggle_subtitle("Arabic");
    let menu = source_menu(&list, &filter);
    assert_eq!(indexes(&menu), [0, 2], "the playing source stays listed");
    assert_eq!(menu.subtitle_shown, 2);
}

#[test]
fn source_menu_flags_preferred_languages_from_stream_and_title_files() {
    let links = vec![
        (link("own", 1080), working(None, &["ara"])),
        (link("none", 1080), working(None, &[])),
    ];
    let shared = vec![SubtitleTrack {
        lang: "English".into(),
        ..SubtitleTrack::default()
    }];
    let preferred = languages(&["English", "Arabic", "French"]);
    let list = SourceList {
        links: &links,
        subtitles: &shared,
        selected: 0,
        preferred: &preferred,
    };
    let menu = source_menu(&list, &SourceFilter::default());
    let flags: Vec<Vec<String>> = menu.rows.iter().map(|row| row.flags.clone()).collect();
    assert_eq!(
        flags,
        [vec!["EN", "AR"], vec!["EN"]],
        "in the viewer's order"
    );
}

#[test]
fn file_size_text_uses_binary_units() {
    assert_eq!(file_size_text(512), "512 B");
    assert_eq!(file_size_text(1536), "1.5 KB");
    assert_eq!(file_size_text(734_003_200), "700.0 MB");
    assert_eq!(file_size_text(1_400_000_000), "1.3 GB");
}

#[test]
fn language_matches_codes_regions_and_names() {
    let english = player_api::ENGLISH;
    assert!(language_matches("en-US", &english));
    assert!(language_matches("ENG", &english));
    assert!(language_matches("English", &english));
    assert!(language_matches("english sdh", &english));
    assert!(!language_matches("Englishman", &english));
    assert!(!language_matches("ara", &english));
}
