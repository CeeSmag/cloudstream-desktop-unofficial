//! Tells a click on the video from the first half of a double click.
//!
//! One double click reaches the screen as two clicks. Pausing on each would pause and resume the
//! video a moment apart, so a click waits for [`DOUBLE_CLICK`]: a second click inside it is a
//! double click (fullscreen, no pause), and when nothing follows the first click pauses. The
//! window is ours instead of Slint's `double-clicked`, so the two cannot disagree on timing.
//! (Ported from the old app's `clickSorter.ts`.)

use std::time::{Duration, Instant};

/// How long a click waits for a second one.
pub(crate) const DOUBLE_CLICK: Duration = Duration::from_millis(250);

/// What a click means right away.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ClickAction {
    /// The first click: it may still become a double click ([`ClickSorter::due`] decides).
    Wait,
    /// The second click inside the window.
    DoubleClick,
}

/// The click waiting for a possible second one.
#[derive(Debug, Default)]
pub(crate) struct ClickSorter {
    pending: Option<Instant>,
}

impl ClickSorter {
    /// A click on the video at `now`.
    pub(crate) fn click(&mut self, now: Instant) -> ClickAction {
        match self.pending.take() {
            Some(first) if now.duration_since(first) < DOUBLE_CLICK => ClickAction::DoubleClick,
            _ => {
                self.pending = Some(now);
                ClickAction::Wait
            }
        }
    }

    /// True once, when a first click has waited long enough: it was a single click.
    pub(crate) fn due(&mut self, now: Instant) -> bool {
        let over = self
            .pending
            .is_some_and(|first| now.duration_since(first) >= DOUBLE_CLICK);
        if over {
            self.pending = None;
        }
        over
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn a_lone_click_becomes_due_after_the_window() {
        let start = Instant::now();
        let mut sorter = ClickSorter::default();
        assert_eq!(sorter.click(start), ClickAction::Wait);
        assert!(!sorter.due(at(start, 249)), "still waiting");
        assert!(sorter.due(at(start, 250)));
        assert!(!sorter.due(at(start, 300)), "reported once");
    }

    #[test]
    fn a_second_click_inside_the_window_is_a_double_click_and_never_due() {
        let start = Instant::now();
        let mut sorter = ClickSorter::default();
        assert_eq!(sorter.click(start), ClickAction::Wait);
        assert_eq!(sorter.click(at(start, 120)), ClickAction::DoubleClick);
        assert!(
            !sorter.due(at(start, 1000)),
            "no pause after a double click"
        );
    }

    #[test]
    fn a_second_click_after_the_window_is_a_new_single_click() {
        let start = Instant::now();
        let mut sorter = ClickSorter::default();
        assert_eq!(sorter.click(start), ClickAction::Wait);
        assert_eq!(sorter.click(at(start, 400)), ClickAction::Wait);
        assert!(sorter.due(at(start, 650)));
    }

    #[test]
    fn a_third_click_starts_over() {
        let start = Instant::now();
        let mut sorter = ClickSorter::default();
        assert_eq!(sorter.click(start), ClickAction::Wait);
        assert_eq!(sorter.click(at(start, 100)), ClickAction::DoubleClick);
        assert_eq!(sorter.click(at(start, 200)), ClickAction::Wait);
        assert!(sorter.due(at(start, 450)));
    }

    #[test]
    fn nothing_is_due_without_a_click() {
        let mut sorter = ClickSorter::default();
        assert!(!sorter.due(Instant::now()));
    }
}
