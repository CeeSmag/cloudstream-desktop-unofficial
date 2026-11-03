use super::*;

const NOW: u64 = 1_000_000;

fn judge_aged(seconds: u64) -> Verdict {
    CacheLimits::default().judge(NOW - seconds, NOW)
}

#[test]
fn one_second_before_twenty_minutes_is_still_fresh() {
    assert_eq!(
        judge_aged(19 * 60 + 59),
        Verdict::Usable(Freshness::Fresh, Some(Duration::from_secs(19 * 60 + 59)))
    );
}

#[test]
fn exactly_twenty_minutes_is_stale() {
    assert_eq!(
        judge_aged(20 * 60),
        Verdict::Usable(Freshness::Stale, Some(Duration::from_secs(20 * 60)))
    );
}

#[test]
fn exactly_twenty_four_hours_is_still_stale_and_one_second_more_is_expired() {
    assert!(matches!(
        judge_aged(24 * 3600),
        Verdict::Usable(Freshness::Stale, _)
    ));
    assert_eq!(judge_aged(24 * 3600 + 1), Verdict::Expired);
}

#[test]
fn a_title_just_saved_is_fresh() {
    assert!(matches!(
        judge_aged(0),
        Verdict::Usable(Freshness::Fresh, Some(_))
    ));
}

#[test]
fn a_title_saved_in_the_future_is_stale_never_fresh() {
    let limits = CacheLimits::default();
    assert_eq!(
        limits.judge(NOW + 5, NOW),
        Verdict::Usable(Freshness::Stale, None)
    );
    assert_eq!(
        limits.judge(NOW + 24 * 3600, NOW),
        Verdict::Usable(Freshness::Stale, None)
    );
}

#[test]
fn a_title_dated_far_in_the_future_is_expired() {
    let limits = CacheLimits::default();
    assert_eq!(limits.judge(NOW + 24 * 3600 + 1, NOW), Verdict::Expired);
}

#[test]
fn the_defaults_are_the_ones_the_plan_names() {
    let limits = CacheLimits::default();
    assert_eq!(limits.fresh_for, Duration::from_secs(1200));
    assert_eq!(limits.keep_for, Duration::from_secs(86_400));
    assert_eq!(limits.max_titles, 50);
    assert_eq!(limits.max_links, 200);
    assert_eq!(limits.max_file_bytes, 1_048_576);
}
