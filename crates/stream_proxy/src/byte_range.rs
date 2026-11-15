//! What a `Range` header asks of a body whose length is known. Only one range of bytes is
//! understood; anything else is answered with the whole body, which HTTP allows.

/// How to answer a request that may carry a `Range` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeAnswer {
    /// Send everything (no range, or one that is not understood).
    Whole,
    /// Send the bytes `start..=end`.
    Partial { start: usize, end: usize },
    /// The range lies beyond the end of the body.
    Unsatisfiable,
}

/// The two ends of `a-b`, `a-` or `-n` as numbers; `None` when it is none of them.
fn parse_spec(spec: &str) -> Option<(Option<usize>, Option<usize>)> {
    let (first, last) = spec.split_once('-')?;
    let number = |text: &str| {
        (!text.is_empty())
            .then(|| text.parse::<usize>().ok())
            .flatten()
    };
    let (first, last) = (number(first), number(last));
    let empty = |text: &str| text.is_empty();
    let well_formed = (first.is_some() || empty(spec.split('-').next()?))
        && (last.is_some() || spec.ends_with('-'));
    well_formed.then_some((first, last))
}

/// Works out how to answer `header` for a body of `length` bytes.
pub(crate) fn answer_range(header: Option<&str>, length: usize) -> RangeAnswer {
    let Some(spec) = header.and_then(|header| header.trim().strip_prefix("bytes=")) else {
        return RangeAnswer::Whole;
    };
    if spec.contains(',') || length == 0 {
        return RangeAnswer::Whole;
    }
    let last_byte = length - 1;
    match parse_spec(spec.trim()) {
        Some((Some(start), _)) if start >= length => RangeAnswer::Unsatisfiable,
        Some((Some(start), Some(end))) if end >= start => RangeAnswer::Partial {
            start,
            end: end.min(last_byte),
        },
        Some((Some(start), None)) => RangeAnswer::Partial {
            start,
            end: last_byte,
        },
        Some((None, Some(0))) => RangeAnswer::Unsatisfiable,
        Some((None, Some(count))) => RangeAnswer::Partial {
            start: length.saturating_sub(count),
            end: last_byte,
        },
        _ => RangeAnswer::Whole,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partial(start: usize, end: usize) -> RangeAnswer {
        RangeAnswer::Partial { start, end }
    }

    #[test]
    fn answer_range_open_ended_and_closed_ranges_are_partial() {
        assert_eq!(answer_range(Some("bytes=0-"), 100), partial(0, 99));
        assert_eq!(answer_range(Some("bytes=10-"), 100), partial(10, 99));
        assert_eq!(answer_range(Some("bytes=10-19"), 100), partial(10, 19));
        assert_eq!(answer_range(Some(" bytes=0-0 "), 100), partial(0, 0));
    }

    #[test]
    fn answer_range_an_end_past_the_body_is_cut_to_the_body() {
        assert_eq!(answer_range(Some("bytes=90-500"), 100), partial(90, 99));
    }

    #[test]
    fn answer_range_a_suffix_range_counts_from_the_end() {
        assert_eq!(answer_range(Some("bytes=-10"), 100), partial(90, 99));
        assert_eq!(answer_range(Some("bytes=-500"), 100), partial(0, 99));
        assert_eq!(
            answer_range(Some("bytes=-0"), 100),
            RangeAnswer::Unsatisfiable
        );
    }

    #[test]
    fn answer_range_a_start_beyond_the_body_is_unsatisfiable() {
        assert_eq!(
            answer_range(Some("bytes=100-"), 100),
            RangeAnswer::Unsatisfiable
        );
        assert_eq!(
            answer_range(Some("bytes=200-300"), 100),
            RangeAnswer::Unsatisfiable
        );
    }

    #[test]
    fn answer_range_no_header_or_one_that_is_not_understood_gives_the_whole_body() {
        for header in [
            None,
            Some(""),
            Some("bytes="),
            Some("bytes=-"),
            Some("bytes=a-b"),
            Some("bytes=5-2"),
            Some("bytes=0-1,5-9"),
            Some("items=0-5"),
            Some("bytes=--5"),
            Some("bytes=1-2-3"),
        ] {
            assert_eq!(answer_range(header, 100), RangeAnswer::Whole, "{header:?}");
        }
    }

    #[test]
    fn answer_range_an_empty_body_is_always_whole() {
        assert_eq!(answer_range(Some("bytes=0-"), 0), RangeAnswer::Whole);
    }
}
