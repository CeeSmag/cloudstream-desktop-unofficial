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
}
