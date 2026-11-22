use std::time::Duration;

use super::HelperIdle;

#[test]
fn minutes_become_the_duration_the_watcher_takes_and_never_becomes_none() {
    assert_eq!(
        HelperIdle::After(5).as_duration(),
        Some(Duration::from_secs(300))
    );
    assert_eq!(HelperIdle::Never.as_duration(), None);
}

#[test]
fn the_choices_run_from_one_minute_to_never() {
    assert_eq!(HelperIdle::CHOICES.first(), Some(&HelperIdle::After(1)));
    assert_eq!(HelperIdle::CHOICES.last(), Some(&HelperIdle::Never));
}

#[test]
fn text_round_trips_for_storage() {
    for choice in HelperIdle::CHOICES {
        assert_eq!(HelperIdle::parse(&choice.to_text()), Some(choice));
    }
    assert_eq!(HelperIdle::parse("never"), Some(HelperIdle::Never));
    assert_eq!(HelperIdle::parse("NEVER"), Some(HelperIdle::Never));
}

#[test]
fn zero_minutes_and_words_are_not_accepted() {
    assert_eq!(HelperIdle::parse("0"), None);
    assert_eq!(HelperIdle::parse("soon"), None);
    assert_eq!(HelperIdle::parse(""), None);
}
