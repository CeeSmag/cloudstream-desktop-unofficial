//! Making what a database says safe to act on.

use crate::skip_segment::SkipSegment;

/// More segments than this in one video is a wrong answer.
const MAX_SEGMENTS: usize = 8;

/// Keeps the segments that make sense for a video of `duration_ms` (0 when not known), in order.
///
/// Dropped: segments that end before they start, start after the video ends, or cover more than
/// four fifths of the video (skipping those would skip the video). Ends past the end of the video
/// are cut to it. Exact repeats are listed once.
pub(crate) fn clean(segments: Vec<SkipSegment>, duration_ms: u64) -> Vec<SkipSegment> {
    let mut kept: Vec<SkipSegment> = Vec::new();
    for mut segment in segments {
        if duration_ms > 0 {
            if segment.start_ms >= duration_ms {
                continue;
            }
            segment.end_ms = segment.end_ms.min(duration_ms);
            if (segment.end_ms.saturating_sub(segment.start_ms)) * 5 > duration_ms * 4 {
                continue;
            }
        }
        if segment.end_ms <= segment.start_ms || kept.contains(&segment) {
            continue;
        }
        kept.push(segment);
    }
    kept.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
    kept.truncate(MAX_SEGMENTS);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skip_segment::SkipKind;

    fn seg(start_ms: u64, end_ms: u64) -> SkipSegment {
        SkipSegment {
            kind: SkipKind::Intro,
            start_ms,
            end_ms,
        }
    }

    #[test]
    fn backwards_empty_and_repeated_segments_are_dropped_and_the_rest_sorted() {
        let kept = clean(
            vec![
                seg(500, 900),
                seg(100, 200),
                seg(300, 300),
                seg(900, 400),
                seg(100, 200),
            ],
            0,
        );
        assert_eq!(kept, [seg(100, 200), seg(500, 900)]);
    }

    #[test]
    fn an_end_past_the_video_is_cut_and_a_start_past_it_is_dropped() {
        let kept = clean(vec![seg(900, 5000), seg(1000, 1200), seg(1500, 1600)], 1000);
        assert_eq!(kept, [seg(900, 1000)]);
    }

    #[test]
    fn a_segment_covering_most_of_the_video_is_dropped_but_exactly_four_fifths_stays() {
        assert!(clean(vec![seg(0, 801)], 1000).is_empty());
        assert_eq!(clean(vec![seg(0, 800)], 1000), [seg(0, 800)]);
    }

    #[test]
    fn no_more_than_eight_are_kept() {
        let many: Vec<_> = (0..20).map(|n| seg(n * 10, n * 10 + 5)).collect();
        assert_eq!(clean(many, 0).len(), 8);
    }
}
