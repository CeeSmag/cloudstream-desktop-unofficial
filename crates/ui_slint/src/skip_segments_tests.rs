use player_api::Chapter;

use super::*;

fn chapter(title: &str, time_ms: i64) -> Chapter {
    Chapter {
        title: title.to_owned(),
        time_ms,
    }
}

fn segment(kind: SkipKind, start_ms: i64, end_ms: i64) -> Segment {
    Segment {
        kind,
        start_ms,
        end_ms,
    }
}

#[test]
fn kind_of_title_reads_naming_words_and_short_forms() {
    assert_eq!(kind_of_title("Opening"), Some(SkipKind::Intro));
    assert_eq!(kind_of_title("Opening 2"), Some(SkipKind::Intro));
    assert_eq!(kind_of_title("Intro1"), Some(SkipKind::Intro));
    assert_eq!(kind_of_title("Opening Credits"), Some(SkipKind::Intro));
    assert_eq!(kind_of_title("Ending"), Some(SkipKind::Credits));
    assert_eq!(kind_of_title("End Credits"), Some(SkipKind::Credits));
    assert_eq!(kind_of_title("Previously on..."), Some(SkipKind::Recap));
    assert_eq!(kind_of_title("Next Time"), Some(SkipKind::Preview));
    assert_eq!(kind_of_title("OP"), Some(SkipKind::Intro));
    assert_eq!(kind_of_title("ED 2"), Some(SkipKind::Credits));
    assert_eq!(kind_of_title("ED2"), Some(SkipKind::Credits));
}

#[test]
fn kind_of_title_leaves_ordinary_chapters_alone() {
    assert_eq!(kind_of_title("Part A"), None);
    assert_eq!(kind_of_title("Chapter 3"), None);
    assert_eq!(
        kind_of_title("Ed meets the OP squad"),
        None,
        "short forms only as the whole title"
    );
    assert_eq!(kind_of_title("Introduction to physics"), None);
    assert_eq!(kind_of_title("Next"), None);
}

#[test]
fn segments_from_chapters_run_to_the_next_chapter_or_the_end() {
    let chapters = [
        chapter("Part B", 200_000),
        chapter("Opening", 10_000),
        chapter("Part A", 100_000),
        chapter("Ending", 1_300_000),
    ];
    assert_eq!(
        segments_from_chapters(&chapters, 1_400_000),
        [
            segment(SkipKind::Intro, 10_000, 100_000),
            segment(SkipKind::Credits, 1_300_000, 1_400_000)
        ]
    );
    assert_eq!(
        segments_from_chapters(&chapters, 0),
        [segment(SkipKind::Intro, 10_000, 100_000)],
        "the last chapter has no end while the length is unknown"
    );
}

#[test]
fn segments_from_chapters_drop_labels_and_join_neighbours() {
    let chapters = [
        chapter("Opening", 0),
        chapter("Opening 2", 40_000),
        chapter("Part A", 90_000),
        chapter("Preview", 500_000),
        chapter("Logo", 502_000),
    ];
    assert_eq!(
        segments_from_chapters(&chapters, 600_000),
        [segment(SkipKind::Intro, 0, 90_000)],
        "two touching openings become one; a 2 s preview is a label"
    );
}

#[test]
fn skip_prompt_inside_a_part_skips_to_its_end() {
    let parts = [segment(SkipKind::Intro, 10_000, 100_000)];
    assert_eq!(
        skip_prompt(&parts, 50_000, 1_400_000, true),
        Some(SkipPrompt {
            label: "Skip Intro",
            action: PromptAction::Seek(100_000)
        })
    );
    assert_eq!(
        skip_prompt(&parts, 100_000, 1_400_000, true),
        None,
        "the end does not count"
    );
}

#[test]
fn skip_prompt_credits_at_the_end_offer_the_next_episode() {
    let parts = [segment(SkipKind::Credits, 1_300_000, 1_390_000)];
    let next = skip_prompt(&parts, 1_310_000, 1_400_000, true);
    assert_eq!(next.map(|p| p.action), Some(PromptAction::NextEpisode));
    let last = skip_prompt(&parts, 1_310_000, 1_400_000, false);
    assert_eq!(last.map(|p| p.label), Some("Skip Credits"));
}

#[test]
fn skip_prompt_without_parts_offers_next_from_ninety_percent() {
    assert_eq!(skip_prompt(&[], 890_000, 1_000_000, true), None);
    assert_eq!(
        skip_prompt(&[], 900_000, 1_000_000, true).map(|p| p.label),
        Some("Next episode")
    );
    assert_eq!(skip_prompt(&[], 950_000, 1_000_000, false), None);
    assert_eq!(skip_prompt(&[], 950_000, 0, true), None);
}

#[test]
fn merge_segments_chapters_win_for_the_kinds_they_name() {
    let chapters = [segment(SkipKind::Intro, 0, 90_000)];
    let online = [
        segment(SkipKind::Intro, 5_000, 80_000),
        segment(SkipKind::Recap, 100_000, 130_000),
        segment(SkipKind::Credits, 1_300_000, 1_400_000),
    ];
    assert_eq!(
        merge_segments(&chapters, &online),
        [
            segment(SkipKind::Intro, 0, 90_000),
            segment(SkipKind::Recap, 100_000, 130_000),
            segment(SkipKind::Credits, 1_300_000, 1_400_000)
        ]
    );
    assert_eq!(merge_segments(&[], &online).len(), 3);
}

#[test]
fn from_online_maps_kinds_and_drops_empty_parts() {
    let found = [
        skip_times::SkipSegment {
            kind: skip_times::SkipKind::Credits,
            start_ms: 1_000,
            end_ms: 9_000,
        },
        skip_times::SkipSegment {
            kind: skip_times::SkipKind::Intro,
            start_ms: 5_000,
            end_ms: 5_000,
        },
    ];
    assert_eq!(
        from_online(&found),
        [segment(SkipKind::Credits, 1_000, 9_000)]
    );
}

#[test]
fn title_kind_films_anime_and_series() {
    assert_eq!(title_kind("Anime", true), skip_times::TitleKind::Movie);
    assert_eq!(title_kind("Anime", false), skip_times::TitleKind::Anime);
    assert_eq!(title_kind("OVA", false), skip_times::TitleKind::Anime);
    assert_eq!(title_kind("TvSeries", false), skip_times::TitleKind::Series);
}
