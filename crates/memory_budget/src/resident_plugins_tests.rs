use super::ResidentPlugins;

#[test]
fn limit_converts_to_the_count_the_worker_pool_takes() {
    assert_eq!(ResidentPlugins::Limit(2).as_limit(), Some(2));
    assert_eq!(ResidentPlugins::Unlimited.as_limit(), None);
}

#[test]
fn from_limit_treats_none_and_zero_as_unlimited() {
    assert_eq!(
        ResidentPlugins::from_limit(None),
        ResidentPlugins::Unlimited
    );
    assert_eq!(
        ResidentPlugins::from_limit(Some(0)),
        ResidentPlugins::Unlimited
    );
    assert_eq!(
        ResidentPlugins::from_limit(Some(3)),
        ResidentPlugins::Limit(3)
    );
}

#[test]
fn from_limit_saturates_instead_of_wrapping() {
    assert_eq!(
        ResidentPlugins::from_limit(Some(100_000)),
        ResidentPlugins::Limit(u8::MAX)
    );
}

#[test]
fn text_round_trips_for_storage() {
    for choice in ResidentPlugins::CHOICES {
        assert_eq!(ResidentPlugins::parse(&choice.to_text()), Some(choice));
    }
    assert_eq!(
        ResidentPlugins::parse("unlimited"),
        Some(ResidentPlugins::Unlimited)
    );
    assert_eq!(ResidentPlugins::parse("many"), None);
}
