use super::ProfileChoice;
use crate::MemoryTier;

#[test]
fn auto_follows_the_detected_tier_and_a_named_profile_ignores_it() {
    assert_eq!(
        ProfileChoice::Auto.tier(MemoryTier::HighEnd),
        MemoryTier::HighEnd
    );
    assert_eq!(
        ProfileChoice::Auto.tier(MemoryTier::LowEnd),
        MemoryTier::LowEnd
    );
    assert_eq!(
        ProfileChoice::LowEnd.tier(MemoryTier::HighEnd),
        MemoryTier::LowEnd
    );
    assert_eq!(
        ProfileChoice::HighEnd.tier(MemoryTier::LowEnd),
        MemoryTier::HighEnd
    );
}

#[test]
fn text_round_trips_for_storage() {
    for choice in ProfileChoice::ALL {
        assert_eq!(ProfileChoice::parse(choice.as_text()), Some(choice));
    }
}

#[test]
fn an_unknown_stored_profile_is_not_accepted() {
    assert_eq!(ProfileChoice::parse("turbo"), None);
    assert_eq!(ProfileChoice::parse(""), None);
}

#[test]
fn auto_is_the_default() {
    assert_eq!(ProfileChoice::default(), ProfileChoice::Auto);
}
