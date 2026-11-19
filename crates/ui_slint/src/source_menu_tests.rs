use app_services::LinkHealth;
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
fn source_menu_lists_the_best_first_and_says_kind_and_size() {
    let mut hls = link("hls", 720);
    hls.link_type = "M3U8".into();
    let links = vec![
        (hls, working(None, &[])),
        (link("uhd", 2160), working(Some(1_400_000_000), &[])),
    ];
    let list = SourceList {
        links: &links,
        subtitles: &[],
        selected: 0,
        preferred: &[],
    };
    let menu = source_menu(&list, &SourceFilter::default());
    assert_eq!(indexes(&menu), [1, 0]);
    let first = menu.rows.first();
    assert_eq!(first.map(|row| row.size.as_str()), Some("1.3 GB"));
    assert_eq!(menu.rows.get(1).map(|row| row.kind.as_str()), Some("HLS"));
    assert_eq!(menu.rows.get(1).map(|row| row.size.as_str()), Some(""));
}

#[test]
fn source_menu_resolution_filter_counts_groups_and_puts_biggest_first() {
    let links = vec![
        (link("small", 1080), working(Some(700_000_000), &[])),
        (link("unknown", 0), working(Some(9_000_000_000), &[])),
        (link("big", 720), working(Some(2_000_000_000), &[])),
        (link("uhd", 2160), working(None, &[])),
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
    assert_eq!(menu.hd_count, 3);
    assert_eq!(menu.uhd_count, 1);
    assert_eq!(
        indexes(&menu),
        [2, 0, 4],
        "sized biggest first, then unsized"
    );
    assert_eq!(menu.resolution_shown, 3);
    assert!(
        !indexes(&menu).contains(&3),
        "the playing source is not kept: it would look like a match"
    );
    filter.toggle_tier(Tier::Hd);
    assert!(filter.tiers.is_empty(), "a second press turns it off");
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
