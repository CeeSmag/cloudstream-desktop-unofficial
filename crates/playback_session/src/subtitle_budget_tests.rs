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
    assert!(!budget.is_preferred_satisfied());

    budget.on_ready("Arabic");
    assert!(!budget.is_preferred_satisfied());

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
    assert!(!budget.accept_cached(&sub("English", "https://c/1.vtt")));
    assert!(!budget.accept_cached(&sub("English", "   ")));
    assert!(!budget.is_preferred_satisfied());
    assert!(budget.accept_cached(&sub("en", "https://c/2.vtt")));
    assert!(budget.is_preferred_satisfied());
}

#[test]
fn empty_preferred_uses_combined_other_cap_and_caps_reserve_backlog() {
    let mut budget = QueueBudget::new(Vec::new(), HashSet::new());
    let total = MAX_PREFERRED_EXTERNAL + MAX_OTHER_EXTERNAL + MAX_RESERVE_TRACKS + 10;
    let tracks: Vec<_> = (0..total)
        .map(|i| sub(&format!("Lang{i}"), &format!("https://h.test/{i}.vtt")))
        .collect();
    let accepted = budget.filter_accepted(&tracks);
    assert_eq!(accepted.len(), MAX_PREFERRED_EXTERNAL + MAX_OTHER_EXTERNAL);
    assert_eq!(budget.reserve.len(), MAX_RESERVE_TRACKS);
    assert!(!budget.accept_cached(&sub("Extra", "https://h.test/extra.vtt")));
}

#[test]
fn release_slot_clears_zero_other_bucket_and_unknown_languages_share_bucket() {
    let mut budget = QueueBudget::new(vec![ENGLISH], HashSet::new());
    let t1 = sub("Klingon (SDH)", "https://bad.test/k1.vtt");
    let t2 = sub("Klingon - Forced", "https://bad.test/k2.vtt");
    let t3 = sub("Klingon", "https://good.test/k3.vtt");
    let accepted = budget.filter_accepted(&[t1.clone(), t2.clone(), t3]);
    assert_eq!(accepted.len(), MAX_PER_OTHER_LANGUAGE);

    let p1 = budget.on_failed(&t1);
    assert_eq!(p1.len(), 1);
    assert_eq!(p1[0].url, "https://good.test/k3.vtt");
    let p2 = budget.on_failed(&t2);
    assert!(p2.is_empty());
    assert!(budget.on_failed(&p1[0]).is_empty());
    assert!(budget.on_failed(&t1).is_empty());

    let next = budget.filter_accepted(&[
        sub("Klingon", "https://bad.test/k4.vtt"),
        sub("Klingon", "https://fresh.test/k5.vtt"),
    ]);
    assert_eq!(next.len(), 2);
    assert_eq!(next[0].url, "https://fresh.test/k5.vtt");
}

#[test]
fn partial_other_failure_keeps_remaining_per_language_count_and_preserves_fifo_order() {
    let mut budget = QueueBudget::new(vec![ENGLISH], HashSet::new());
    let fr1 = sub("French", "https://f.test/1.vtt");
    let fr2 = sub("French", "https://f.test/2.vtt");
    assert_eq!(budget.filter_accepted(&[fr1.clone(), fr2]).len(), 2);

    // Failing 1 of 2 French tracks leaves count=1, so only 1 new French track fits (not 2).
    assert!(budget.on_failed(&fr1).is_empty());
    let added = budget.filter_accepted(&[
        sub("French", "https://f.test/3.vtt"),
        sub("French", "https://f.test/4.vtt"),
        sub("French", "https://f.test/5.vtt"),
    ]);
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].url, "https://f.test/3.vtt");
    // Equal-priority reserve tracks must promote in FIFO discovery order (4 before 5).
    let promoted = budget.on_failed(&added[0]);
    assert_eq!(promoted.len(), 1);
    assert_eq!(promoted[0].url, "https://f.test/4.vtt");
}

#[test]
fn reserve_promotion_preserves_ineligible_preferred_and_per_lang_candidates() {
    let mut budget = QueueBudget::new(vec![ENGLISH], HashSet::new());
    let mut tracks: Vec<_> = (0..MAX_PREFERRED_EXTERNAL)
        .map(|i| sub("English", &format!("https://en.test/{i}.vtt")))
        .collect();
    tracks.push(sub("English", "https://en.test/reserve.vtt"));
    let fr1 = sub("French", "https://fr.test/1.vtt");
    let fr2 = sub("French", "https://fr.test/2.vtt");
    tracks.push(fr1.clone());
    tracks.push(fr2);
    tracks.push(sub("French", "https://fr.test/reserve.vtt"));

    let initial = budget.filter_accepted(&tracks);
    let en0 = initial[0].clone();

    // Failing French must promote French reserve while keeping English reserve untouched.
    let fr_promoted = budget.on_failed(&fr1);
    assert_eq!(fr_promoted.len(), 1);
    assert_eq!(fr_promoted[0].url, "https://fr.test/reserve.vtt");

    // Now failing English must still find the preserved English reserve (and NOT discard French).
    let en_promoted = budget.on_failed(&en0);
    assert_eq!(en_promoted.len(), 1);
    assert_eq!(en_promoted[0].url, "https://en.test/reserve.vtt");
}

#[test]
fn reserve_promotion_preserves_other_tracks_blocked_by_total_or_per_lang_cap() {
    let mut budget = QueueBudget::new(vec![ENGLISH], HashSet::new());
    let mut tracks = vec![
        sub("English", "https://en.test/0.vtt"),
        sub("English", "https://en.test/1.vtt"),
    ];
    // Fill MAX_OTHER_EXTERNAL (12) across 6 non-preferred languages (2 each).
    for lang_idx in 0..(MAX_OTHER_EXTERNAL / MAX_PER_OTHER_LANGUAGE) {
        for copy in 0..MAX_PER_OTHER_LANGUAGE {
            tracks.push(sub(
                &format!("Other{lang_idx}"),
                &format!("https://o.test/{lang_idx}_{copy}.vtt"),
            ));
        }
    }
    // Blocked ONLY by per_lang == MAX_PER_OTHER_LANGUAGE once one other slot frees (placed first
    // on a healthy host so an off-by-one `per_lang <= cap` would pop and drop it before Spanish).
    tracks.push(sub("Other0", "https://o0-reserve.test/0_reserve.vtt"));
    // Blocked ONLY by other_count == MAX_OTHER_EXTERNAL (per_lang == 0 < 2).
    tracks.push(sub("Spanish", "https://es.test/reserve.vtt"));

    let accepted = budget.filter_accepted(&tracks);
    let en0 = accepted[0].clone();
    let other1 = sub("Other1", "https://o.test/1_0.vtt");

    // Failing English while other_count is full must not drop Spanish or Other0 from reserve.
    assert!(budget.on_failed(&en0).is_empty());
    // Failing Other1 frees 1 other_count slot (Other0 still has per_lang=2): Spanish wins, Other0 stays!
    let promoted = budget.on_failed(&other1);
    assert_eq!(promoted.len(), 1);
    assert_eq!(promoted[0].url, "https://es.test/reserve.vtt");

    // Now failing Other0 frees both other_count and Other0's per_lang slot: Other0 reserve promotes!
    let promoted2 = budget.on_failed(&sub("Other0", "https://o.test/0_0.vtt"));
    assert_eq!(promoted2.len(), 1);
    assert_eq!(promoted2[0].url, "https://o0-reserve.test/0_reserve.vtt");
}
