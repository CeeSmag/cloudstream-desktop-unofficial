use super::HelperWindows;

#[test]
fn the_choices_are_one_two_four_and_eight_windows() {
    let counts: Vec<usize> = HelperWindows::CHOICES.iter().map(|c| c.count()).collect();
    assert_eq!(counts, vec![1, 2, 4, 8]);
}

#[test]
fn text_round_trips_for_storage() {
    for choice in HelperWindows::CHOICES {
        assert_eq!(HelperWindows::parse(&choice.to_text()), Some(choice));
    }
}

#[test]
fn zero_words_and_unoffered_counts_are_not_accepted() {
    assert_eq!(HelperWindows::parse("0"), None);
    assert_eq!(HelperWindows::parse("3"), None);
    assert_eq!(HelperWindows::parse("many"), None);
    assert_eq!(HelperWindows::parse(""), None);
}

#[test]
fn a_count_is_read_with_spaces_around_it() {
    assert_eq!(HelperWindows::parse(" 4 "), Some(HelperWindows::new(4)));
}
