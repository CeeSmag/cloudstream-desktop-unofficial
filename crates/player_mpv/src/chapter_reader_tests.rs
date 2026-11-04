use std::collections::HashMap;

use super::*;

fn properties(entries: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = entries
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

fn chapter(title: &str, time_ms: i64) -> Chapter {
    Chapter {
        title: title.to_owned(),
        time_ms,
    }
}

#[test]
fn chapters_come_back_with_their_names_and_times_in_milliseconds() {
    let get = properties(&[
        ("chapter-list/count", "2"),
        ("chapter-list/0/title", "Opening"),
        ("chapter-list/0/time", "0.000000"),
        ("chapter-list/1/title", "Part A"),
        ("chapter-list/1/time", "93.4567"),
    ]);
    assert_eq!(
        read_chapters(get),
        [chapter("Opening", 0), chapter("Part A", 93_457)]
    );
}

#[test]
fn a_chapter_without_a_name_is_called_chapter_n() {
    let get = properties(&[
        ("chapter-list/count", "3"),
        ("chapter-list/0/time", "0"),
        ("chapter-list/1/title", "   "),
        ("chapter-list/1/time", "10"),
        ("chapter-list/2/title", "End"),
        ("chapter-list/2/time", "20"),
    ]);
    let titles: Vec<_> = read_chapters(get).into_iter().map(|c| c.title).collect();
    assert_eq!(titles, ["Chapter 1", "Chapter 2", "End"]);
}

#[test]
fn a_chapter_with_no_usable_time_is_left_out_and_the_rest_are_sorted() {
    let get = properties(&[
        ("chapter-list/count", "5"),
        ("chapter-list/0/title", "late"),
        ("chapter-list/0/time", "50"),
        ("chapter-list/1/title", "garbled"),
        ("chapter-list/1/time", "soon"),
        ("chapter-list/2/title", "negative"),
        ("chapter-list/2/time", "-3"),
        ("chapter-list/3/title", "early"),
        ("chapter-list/3/time", "5"),
        ("chapter-list/4/title", "missing time"),
    ]);
    assert_eq!(
        read_chapters(get),
        [chapter("early", 5000), chapter("late", 50_000)]
    );
}

#[test]
fn no_chapters_or_an_unreadable_count_gives_an_empty_list() {
    assert!(read_chapters(properties(&[])).is_empty());
    assert!(read_chapters(properties(&[("chapter-list/count", "0")])).is_empty());
    assert!(read_chapters(properties(&[("chapter-list/count", "many")])).is_empty());
}

#[test]
fn nan_and_infinite_times_are_refused() {
    for bad in ["NaN", "inf", "-inf"] {
        let get = properties(&[("chapter-list/count", "1"), ("chapter-list/0/time", bad)]);
        assert!(read_chapters(get).is_empty(), "{bad}");
    }
}
