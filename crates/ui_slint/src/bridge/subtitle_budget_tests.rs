//! Unit tests for `subtitle_budget`: failure refunding, healthy-host reserve promotion, and
//! per-preferred-language satisfaction thresholds.

use std::collections::HashSet;

use core_model::SubtitleTrack;
use player_api::{find_language, ENGLISH};

use super::*;

fn sub(lang: &str, url: &str) -> SubtitleTrack {
    SubtitleTrack {
        lang: lang.to_owned(),
        url: url.to_owned(),
        headers: None,
    }
}

#[test]
fn matches_subtitle_label_handles_iso_codes_and_qualified_names() {
    let arabic = find_language("ara").expect("Arabic");
    assert!(matches_subtitle_label("en", &ENGLISH));
    assert!(matches_subtitle_label("eng", &ENGLISH));
    assert!(matches_subtitle_label("en-US", &ENGLISH));
    assert!(matches_subtitle_label("English (SDH)", &ENGLISH));
    assert!(matches_subtitle_label("English - Forced", &ENGLISH));
    assert!(matches_subtitle_label("Arabic [CC]", &arabic));
    assert!(!matches_subtitle_label("French", &ENGLISH));
    assert!(!matches_subtitle_label("", &ENGLISH));
}

#[test]
fn budget_prioritizes_preferred_and_caps_non_preferred_per_language() {
    let arabic = find_language("ara").expect("Arabic");
    let mut budget = QueueBudget::new(vec![arabic, ENGLISH], HashSet::new());
    let tracks = vec![
        sub("French", "https://s/fr1"),
        sub("French", "https://s/fr2"),
        sub("French", "https://s/fr3"),
        sub("English (SDH)", "https://s/en1"),
        sub("ar", "https://s/ar1"),
        sub("ar", "https://s/ar1"),
    ];
    let accepted = budget.filter_accepted(&tracks);
    let urls: Vec<&str> = accepted.iter().map(|t| t.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://s/ar1",
            "https://s/en1",
            "https://s/fr1",
            "https://s/fr2"
        ]
    );
}

#[test]
fn failed_preferred_track_refunds_slot_and_promotes_healthy_host_from_reserve() {
    let mut budget = QueueBudget::new(vec![ENGLISH], HashSet::new());
    let mut tracks: Vec<SubtitleTrack> = (0..MAX_PREFERRED_EXTERNAL)
        .map(|i| sub("English", &format!("https://dead.test/{i}.vtt")))
        .collect();
    // Reserve contains another dead-host track first, followed by a healthy-host track.
    tracks.push(sub("English", "https://dead.test/reserve-dead.vtt"));
    tracks.push(sub("English", "https://healthy.test/reserve-ok.vtt"));

    let initial = budget.filter_accepted(&tracks);
    assert_eq!(initial.len(), MAX_PREFERRED_EXTERNAL);

    let promoted = budget.on_failed(&initial[0]);
    assert_eq!(promoted.len(), 1);
    assert_eq!(
        promoted[0].url, "https://healthy.test/reserve-ok.vtt",
        "promotion must prefer a healthy host over the failed host"
    );
}

#[test]
fn failed_non_preferred_track_refunds_per_language_cap() {
    let mut budget = QueueBudget::new(vec![ENGLISH], HashSet::new());
    let tracks = vec![
        sub("French", "https://bad.test/fr1.vtt"),
        sub("French", "https://bad.test/fr2.vtt"),
        sub("French", "https://good.test/fr3.vtt"),
    ];
    let initial = budget.filter_accepted(&tracks);
    assert_eq!(initial.len(), MAX_PER_OTHER_LANGUAGE);

    let promoted = budget.on_failed(&initial[0]);
    assert_eq!(promoted.len(), 1);
    assert_eq!(promoted[0].url, "https://good.test/fr3.vtt");
}

#[test]
fn preferred_satisfaction_requires_two_working_for_each_preferred_language() {
    let arabic = find_language("ara").expect("Arabic");
    let mut budget = QueueBudget::new(vec![arabic, ENGLISH], HashSet::new());

    assert!(!budget.is_preferred_satisfied());
    budget.on_ready("English");
    budget.on_ready("English (SDH)");
    budget.on_ready("English");
    // English has 3, Arabic has 0 -> not yet satisfied!
    assert!(!budget.is_preferred_satisfied());

    budget.on_ready("Arabic");
    assert!(!budget.is_preferred_satisfied());

    // Non-preferred languages do not satisfy Arabic.
    budget.on_ready("French");
    budget.on_ready("French");
    assert!(!budget.is_preferred_satisfied());

    budget.on_ready("ar");
    assert!(budget.is_preferred_satisfied());
}

#[test]
fn preferred_satisfaction_falls_back_to_two_any_language_when_no_preferred_set() {
    let mut budget = QueueBudget::new(Vec::new(), HashSet::new());
    assert!(!budget.is_preferred_satisfied());
    budget.on_ready("Spanish");
    assert!(!budget.is_preferred_satisfied());
    budget.on_ready("French");
    assert!(budget.is_preferred_satisfied());
}

#[test]
fn cached_preferred_subtitles_count_toward_satisfaction_immediately() {
    let mut budget = QueueBudget::new(vec![ENGLISH], HashSet::new());
    assert!(budget.accept_cached(&sub("English", "https://c/1.vtt")));
    assert!(!budget.is_preferred_satisfied());
    assert!(budget.accept_cached(&sub("en", "https://c/2.vtt")));
    assert!(budget.is_preferred_satisfied());
}
