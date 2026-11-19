//! When a checked open may start playback: full scan, skip, or time cap — best working by rank.

use app_services::link_rank;
use core_model::PlayableLink;

/// Whether stream `index` (rank `rank`) is a better start than `other` (rank `other_rank`).
pub(crate) fn beats(rank: i32, index: usize, other_rank: i32, other: usize) -> bool {
    rank > other_rank || (rank == other_rank && index < other)
}

/// Best working index so far after stream `index` checked as working.
pub(crate) fn best_after_working(
    best: Option<usize>,
    index: usize,
    links: &[(PlayableLink, Option<app_services::LinkHealth>)],
) -> usize {
    let rank = links
        .get(index)
        .map(|(link, _)| link_rank(link))
        .unwrap_or_default();
    match best {
        Some(current) => {
            let other_rank = links
                .get(current)
                .map(|(link, _)| link_rank(link))
                .unwrap_or_default();
            if beats(rank, index, other_rank, current) {
                index
            } else {
                current
            }
        }
        None => index,
    }
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
pub(crate) fn no_streams_timed_out(found: usize, elapsed_secs: u64, first_stream_secs: u64) -> bool {
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

    fn working() -> LinkHealth {
        LinkHealth::Working {
            size_bytes: None,
            subtitles: Vec::new(),
        }
    }

    #[test]
    fn a_higher_rank_beats_a_lower_one_and_ties_prefer_the_earlier_index() {
        assert!(beats(1080, 1, 720, 0));
        assert!(!beats(720, 0, 1080, 1));
        assert!(beats(1080, 0, 1080, 2));
        assert!(!beats(1080, 2, 1080, 0));
    }

    #[test]
    fn best_after_working_keeps_the_highest_rank() {
        let links = vec![
            (link(720, "https://a/low.mp4"), Some(working())),
            (link(1080, "https://a/hi.mp4"), Some(working())),
            (link(480, "https://a/worse.mp4"), Some(working())),
        ];
        let after_low = best_after_working(None, 0, &links);
        assert_eq!(after_low, 0);
        let after_hi = best_after_working(Some(0), 1, &links);
        assert_eq!(after_hi, 1);
        let after_worse = best_after_working(Some(1), 2, &links);
        assert_eq!(after_worse, 1);
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
