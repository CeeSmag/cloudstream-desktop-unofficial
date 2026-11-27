//! When a checked open may start playback: full scan, skip, or time cap — best working by
//! preferred resolution and file size (with `link_rank` and index as tie-breakers).

use std::cmp::Reverse;

use app_services::{link_rank, LinkHealth, PreferredResolution};
use core_model::PlayableLink;

fn size_of(health: Option<&LinkHealth>) -> Option<u64> {
    match health {
        Some(LinkHealth::Working { size_bytes, .. }) => *size_bytes,
        _ => None,
    }
}

/// Priority key for probing an unchecked stream (newtype avoids tuple mutation explosion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CheckPriorityKey(bool, bool, i32, Reverse<usize>);

/// Comparison key for a working stream (newtype avoids tuple mutation explosion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct WorkingScore(bool, bool, Option<u64>, i32, Reverse<usize>);

/// Priority for probing an unchecked stream: video first, preferred resolution first, then higher
/// `link_rank`, with earlier index breaking ties.
pub(crate) fn check_priority_key(
    link: &PlayableLink,
    index: usize,
    preferred: PreferredResolution,
) -> CheckPriorityKey {
    let rank = link_rank(link);
    CheckPriorityKey(
        rank >= 0,
        preferred.matches_quality(link.quality),
        rank,
        Reverse(index),
    )
}

/// Comparison key for a working stream: video over audio-only, preferred resolution first, largest
/// known file size first, then higher `link_rank`, then earlier index.
fn working_score(
    link: &PlayableLink,
    health: Option<&LinkHealth>,
    index: usize,
    preferred: PreferredResolution,
) -> WorkingScore {
    let rank = link_rank(link);
    WorkingScore(
        rank >= 0,
        preferred.matches_quality(link.quality),
        size_of(health),
        rank,
        Reverse(index),
    )
}

/// Best working index so far after stream `index` checked as working.
pub(crate) fn best_after_working(
    best: Option<usize>,
    index: usize,
    links: &[(PlayableLink, Option<LinkHealth>)],
    preferred: PreferredResolution,
) -> usize {
    let Some(current) = best else {
        return index;
    };
    let score = |slot: usize| {
        links
            .get(slot)
            .map(|(link, health)| working_score(link, health.as_ref(), slot, preferred))
    };
    [current, index]
        .into_iter()
        .max_by_key(|&slot| score(slot))
        .unwrap_or(current)
}

/// Flags that decide whether playback may start.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReadyWhen {
    pub(crate) has_working: bool,
    pub(crate) search_done: bool,
    pub(crate) all_checked: bool,
    pub(crate) skip: bool,
    pub(crate) total_timed_out: bool,
}

/// Start now when there is a working stream and the wait is over (complete, skip, or total cap).
pub(crate) fn should_announce_ready(when: ReadyWhen) -> bool {
    if !when.has_working {
        return false;
    }
    when.skip || when.total_timed_out || (when.search_done && when.all_checked)
}

/// No candidate at all after the first-stream wait: fail the open.
pub(crate) fn no_streams_timed_out(
    found: usize,
    elapsed_secs: u64,
    first_stream_secs: u64,
) -> bool {
    found == 0 && elapsed_secs >= first_stream_secs
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_services::LinkHealth;
    use core_model::PlayableLink;

    fn link(quality: i32, url: &str) -> PlayableLink {
        PlayableLink {
            url: url.to_owned(),
            quality,
            link_type: "VIDEO".into(),
            ..PlayableLink::default()
        }
    }

    fn working(size_bytes: Option<u64>) -> Option<LinkHealth> {
        Some(LinkHealth::Working {
            size_bytes,
            subtitles: Vec::new(),
        })
    }

    #[test]
    fn check_priority_key_prefers_matching_resolution_then_rank_then_index() {
        let hd = link(1080, "https://a/1080.mp4");
        let uhd = link(2160, "https://a/2160.mp4");
        let audio = link(1080, "https://a/audio.mp3");
        let unknown_video = link(0, "https://a/video.mp4");
        assert!(
            check_priority_key(&uhd, 1, PreferredResolution::Any)
                > check_priority_key(&hd, 0, PreferredResolution::Any)
        );
        assert!(
            check_priority_key(&hd, 1, PreferredResolution::Hd)
                > check_priority_key(&uhd, 0, PreferredResolution::Hd)
        );
        assert!(
            check_priority_key(&hd, 0, PreferredResolution::Hd)
                > check_priority_key(&hd, 2, PreferredResolution::Hd)
        );
        assert!(
            check_priority_key(&unknown_video, 1, PreferredResolution::Hd)
                > check_priority_key(&audio, 0, PreferredResolution::Hd),
            "even a non-preferred video stream must be probed before an audio-only stream"
        );
    }

    #[test]
    fn best_after_working_prefers_largest_file_size_when_any() {
        let links = vec![
            (link(2160, "https://a/small-4k.mp4"), working(Some(900_000))),
            (
                link(1080, "https://a/big-1080.mp4"),
                working(Some(3_000_000)),
            ),
            (link(720, "https://a/mid-720.mp4"), working(Some(1_500_000))),
        ];
        let best = best_after_working(None, 0, &links, PreferredResolution::Any);
        let best = best_after_working(Some(best), 1, &links, PreferredResolution::Any);
        let best = best_after_working(Some(best), 2, &links, PreferredResolution::Any);
        assert_eq!(best, 1);
    }

    #[test]
    fn best_after_working_prefers_preferred_resolution_with_largest_file() {
        let links = vec![
            (
                link(2160, "https://a/huge-4k.mp4"),
                working(Some(9_000_000_000)),
            ),
            (
                link(1080, "https://a/small-hd.mp4"),
                working(Some(1_000_000_000)),
            ),
            (
                link(720, "https://a/big-hd.mp4"),
                working(Some(2_500_000_000)),
            ),
        ];
        let best = best_after_working(None, 0, &links, PreferredResolution::Hd);
        let best = best_after_working(Some(best), 1, &links, PreferredResolution::Hd);
        let best = best_after_working(Some(best), 2, &links, PreferredResolution::Hd);
        assert_eq!(best, 2, "largest file within 1080p group wins over 2160p");
    }

    #[test]
    fn best_after_working_falls_back_to_largest_overall_when_preferred_absent() {
        let links = vec![
            (link(720, "https://a/small.mp4"), working(Some(800_000_000))),
            (
                link(1080, "https://a/big.mp4"),
                working(Some(2_000_000_000)),
            ),
        ];
        let best = best_after_working(None, 0, &links, PreferredResolution::Uhd);
        let best = best_after_working(Some(best), 1, &links, PreferredResolution::Uhd);
        assert_eq!(
            best, 1,
            "when no 2160p stream exists, falls back to largest working stream"
        );
    }

    #[test]
    fn best_after_working_tiebreaks_unknown_or_equal_sizes_by_rank_then_index() {
        let links = vec![
            (link(720, "https://a/low.mp4"), working(None)),
            (link(1080, "https://a/hi-a.mp4"), working(None)),
            (link(1080, "https://a/hi-b.mp4"), working(None)),
        ];
        let best = best_after_working(None, 0, &links, PreferredResolution::Any);
        let best = best_after_working(Some(best), 1, &links, PreferredResolution::Any);
        let best = best_after_working(Some(best), 2, &links, PreferredResolution::Any);
        assert_eq!(best, 1);
    }

    #[test]
    fn best_after_working_prefers_video_over_larger_audio_only() {
        let links = vec![
            (link(1080, "https://a/audio.mp3"), working(Some(50_000_000))),
            (link(720, "https://a/video.m3u8"), working(None)),
        ];
        let best = best_after_working(None, 0, &links, PreferredResolution::Hd);
        let best = best_after_working(Some(best), 1, &links, PreferredResolution::Hd);
        assert_eq!(best, 1);
    }

    #[test]
    fn working_stream_does_not_announce_while_search_or_checks_go_on() {
        let mid = ReadyWhen {
            has_working: true,
            search_done: false,
            all_checked: false,
            skip: false,
            total_timed_out: false,
        };
        assert!(!should_announce_ready(mid));
        assert!(!should_announce_ready(ReadyWhen {
            search_done: true,
            ..mid
        }));
        assert!(!should_announce_ready(ReadyWhen {
            all_checked: true,
            ..mid
        }));
    }

    #[test]
    fn announces_on_complete_skip_or_total_cap_when_something_works() {
        assert!(should_announce_ready(ReadyWhen {
            has_working: true,
            search_done: true,
            all_checked: true,
            skip: false,
            total_timed_out: false,
        }));
        assert!(should_announce_ready(ReadyWhen {
            has_working: true,
            search_done: false,
            all_checked: false,
            skip: true,
            total_timed_out: false,
        }));
        assert!(should_announce_ready(ReadyWhen {
            has_working: true,
            search_done: false,
            all_checked: false,
            skip: false,
            total_timed_out: true,
        }));
        assert!(!should_announce_ready(ReadyWhen {
            has_working: false,
            search_done: true,
            all_checked: true,
            skip: true,
            total_timed_out: true,
        }));
    }

    #[test]
    fn no_streams_timed_out_only_when_empty_past_the_first_wait() {
        assert!(!no_streams_timed_out(0, 29, 30));
        assert!(no_streams_timed_out(0, 30, 30));
        assert!(!no_streams_timed_out(1, 60, 30));
    }
}
