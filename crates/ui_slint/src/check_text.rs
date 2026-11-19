//! The words and the bar of the panel shown while a video's streams are found and checked.
//!
//! Ported from the old app (`checkText.ts`, `SourceCheckPanel.tsx`).

use playback_session::OpenProgress;

/// The headline: what the opening is doing.
pub(crate) fn check_title(progress: &OpenProgress) -> &'static str {
    if progress.found == 0 {
        "Finding sources…"
    } else if progress.search_done {
        "Checking sources…"
    } else {
        "Finding and checking sources…"
    }
}

/// The counts under the headline; empty while nothing is found yet.
pub(crate) fn check_detail(progress: &OpenProgress) -> String {
    if progress.found == 0 {
        return String::new();
    }
    let counts = format!(
        "{} of {} checked · {} working",
        progress.checked, progress.found, progress.working
    );
    if progress.search_done {
        counts
    } else {
        format!("{counts} · still searching")
    }
}

/// How much of the check is done, 0 to 1.
pub(crate) fn check_fraction(progress: &OpenProgress) -> f32 {
    if progress.found == 0 {
        return 0.0;
    }
    (progress.checked as f32 / progress.found as f32).clamp(0.0, 1.0)
}

/// Whether "Play now" can start: a stream is known to work.
pub(crate) fn can_skip_check(progress: &OpenProgress) -> bool {
    progress.working > 0
}

/// Compact status line shown inside the player's Sources panel while sources load or refresh.
pub(crate) fn sources_loading_text(progress: &OpenProgress, refreshing: bool) -> String {
    if progress.found == 0 {
        return if refreshing {
            "Refreshing sources…"
        } else {
            "Finding sources…"
        }
        .to_owned();
    }
    if !progress.search_done {
        let verb = if refreshing {
            "Refreshing sources"
        } else {
            "Finding sources"
        };
        return format!(
            "{verb} · {} of {} checked",
            progress.checked, progress.found
        );
    }
    if progress.checked < progress.found {
        format!(
            "Checking sources… {} of {}",
            progress.checked, progress.found
        )
    } else {
        "Finishing sources…".to_owned()
    }
}

/// Compact status line shown inside the player's Subtitles panel while subtitles load in.
/// Returns `None` once search, download, and player attachment have all finished.
pub(crate) fn subtitles_loading_text(
    searching: bool,
    downloads: crate::bridge::SubtitleLoadSnapshot,
    mpv_pending: usize,
) -> Option<String> {
    if downloads.busy {
        let done = downloads.ready + downloads.failed;
        return if downloads.submitted > 0 {
            Some(format!(
                "Loading subtitles… ({done} of {})",
                downloads.submitted
            ))
        } else {
            Some("Loading subtitles…".to_owned())
        };
    }
    if mpv_pending > 0 {
        return Some("Attaching subtitles…".to_owned());
    }
    if searching {
        return Some("Searching for subtitles…".to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(found: usize, checked: usize, working: usize, search_done: bool) -> OpenProgress {
        OpenProgress {
            found,
            checked,
            working,
            search_done,
        }
    }

    #[test]
    fn check_title_and_detail_follow_the_search() {
        let none = progress(0, 0, 0, false);
        assert_eq!(check_title(&none), "Finding sources…");
        assert_eq!(check_detail(&none), "");
        let busy = progress(12, 3, 1, false);
        assert_eq!(check_title(&busy), "Finding and checking sources…");
        assert_eq!(
            check_detail(&busy),
            "3 of 12 checked · 1 working · still searching"
        );
        let done = progress(12, 5, 2, true);
        assert_eq!(check_title(&done), "Checking sources…");
        assert_eq!(check_detail(&done), "5 of 12 checked · 2 working");
    }

    #[test]
    fn check_fraction_and_play_now() {
        assert!(check_fraction(&progress(0, 0, 0, false)).abs() < 1e-6);
        assert!((check_fraction(&progress(4, 1, 0, false)) - 0.25).abs() < 1e-6);
        assert!(!can_skip_check(&progress(4, 1, 0, false)));
        assert!(can_skip_check(&progress(4, 2, 1, false)));
    }

    #[test]
    fn sources_loading_text_distinguishes_initial_and_refresh() {
        assert_eq!(
            sources_loading_text(&progress(0, 0, 0, false), false),
            "Finding sources…"
        );
        assert_eq!(
            sources_loading_text(&progress(0, 0, 0, false), true),
            "Refreshing sources…"
        );
        assert_eq!(
            sources_loading_text(&progress(10, 4, 2, false), true),
            "Refreshing sources · 4 of 10 checked"
        );
        assert_eq!(
            sources_loading_text(&progress(10, 6, 3, true), false),
            "Checking sources… 6 of 10"
        );
    }

    #[test]
    fn subtitles_loading_text_covers_download_attach_search_and_idle() {
        let idle = crate::bridge::SubtitleLoadSnapshot::default();
        assert_eq!(subtitles_loading_text(false, idle, 0), None);
        assert_eq!(
            subtitles_loading_text(true, idle, 0).as_deref(),
            Some("Searching for subtitles…")
        );
        assert_eq!(
            subtitles_loading_text(false, idle, 2).as_deref(),
            Some("Attaching subtitles…")
        );
        let busy = crate::bridge::SubtitleLoadSnapshot {
            submitted: 5,
            ready: 2,
            failed: 1,
            busy: true,
        };
        assert_eq!(
            subtitles_loading_text(true, busy, 1).as_deref(),
            Some("Loading subtitles… (3 of 5)")
        );
    }
}
