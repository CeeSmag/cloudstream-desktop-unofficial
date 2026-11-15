//! Decides when the playback diagnostics are due: once, a while after a stream has loaded.
//!
//! The clock is passed in, so the rule is tested without waiting.

use std::time::{Duration, Instant};

/// How long a stream plays before its diagnostics are written: long enough for the decoder, the
/// video output and the frame rate estimate to have settled, short enough to still be useful.
pub(crate) const DIAGNOSTICS_DELAY: Duration = Duration::from_secs(30);

/// The stream the timer is counting for, and since when.
#[derive(Debug, Clone, Copy)]
struct Counting {
    generation: u64,
    since: Instant,
}

/// Fires once per loaded stream, [`DIAGNOSTICS_DELAY`] after the event thread first saw it loaded.
#[derive(Debug)]
pub(crate) struct DiagnosticsTimer {
    delay: Duration,
    counting: Option<Counting>,
    reported: Option<u64>,
}

impl DiagnosticsTimer {
    pub(crate) fn new(delay: Duration) -> Self {
        Self {
            delay,
            counting: None,
            reported: None,
        }
    }

    /// Call on every pass of the event loop with the generation of the stream that is loaded
    /// (`None` while nothing is). Returns true exactly once per loaded stream, when its time is up.
    ///
    /// The time runs from when the loop first saw the stream loaded, so a stream that is paused or
    /// buffering is still reported on schedule; the figures describe the stream, not how long it
    /// was watched. A stream that ends and is opened again is reported again.
    pub(crate) fn is_due(&mut self, now: Instant, loaded: Option<u64>) -> bool {
        let Some(generation) = loaded else {
            self.counting = None;
            self.reported = None;
            return false;
        };
        if self.reported == Some(generation) {
            return false;
        }
        let counting = match self.counting {
            Some(counting) if counting.generation == generation => counting,
            _ => {
                let counting = Counting {
                    generation,
                    since: now,
                };
                self.counting = Some(counting);
                counting
            }
        };
        let due = now.saturating_duration_since(counting.since) >= self.delay;
        if due {
            self.reported = Some(generation);
            self.counting = None;
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Duration = Duration::from_secs(30);

    fn timer() -> (DiagnosticsTimer, Instant) {
        (DiagnosticsTimer::new(DELAY), Instant::now())
    }

    fn after(start: Instant, seconds: u64) -> Instant {
        start + Duration::from_secs(seconds)
    }

    #[test]
    fn is_due_before_the_delay_is_up_returns_false() {
        let (mut timer, start) = timer();
        assert!(!timer.is_due(start, Some(1)));
        assert!(!timer.is_due(after(start, 29), Some(1)));
    }

    #[test]
    fn is_due_when_the_delay_is_up_returns_true_once() {
        let (mut timer, start) = timer();
        assert!(!timer.is_due(start, Some(1)));
        assert!(timer.is_due(after(start, 30), Some(1)));
        assert!(!timer.is_due(after(start, 31), Some(1)));
        assert!(!timer.is_due(after(start, 600), Some(1)));
    }

    #[test]
    fn is_due_without_a_loaded_stream_returns_false_and_never_counts() {
        let (mut timer, start) = timer();
        assert!(!timer.is_due(start, None));
        assert!(!timer.is_due(after(start, 100), None));
        assert!(
            !timer.is_due(after(start, 100), Some(1)),
            "counting starts now"
        );
    }

    #[test]
    fn a_new_stream_restarts_the_clock() {
        let (mut timer, start) = timer();
        assert!(!timer.is_due(start, Some(1)));
        assert!(!timer.is_due(after(start, 25), Some(2)));
        assert!(
            !timer.is_due(after(start, 50), Some(2)),
            "25 s after stream 2 loaded"
        );
        assert!(timer.is_due(after(start, 55), Some(2)));
    }

    #[test]
    fn a_stream_that_ends_before_the_delay_is_never_reported() {
        let (mut timer, start) = timer();
        assert!(!timer.is_due(start, Some(1)));
        assert!(!timer.is_due(after(start, 10), None));
        assert!(!timer.is_due(after(start, 40), None));
    }

    #[test]
    fn the_same_stream_opened_again_after_the_end_is_reported_again() {
        let (mut timer, start) = timer();
        assert!(!timer.is_due(start, Some(1)));
        assert!(timer.is_due(after(start, 30), Some(1)));
        assert!(!timer.is_due(after(start, 31), None), "the file ended");
        assert!(!timer.is_due(after(start, 32), Some(1)));
        assert!(timer.is_due(after(start, 62), Some(1)));
    }

    #[test]
    fn a_zero_delay_is_due_on_the_first_pass() {
        let mut timer = DiagnosticsTimer::new(Duration::ZERO);
        assert!(timer.is_due(Instant::now(), Some(7)));
    }

    #[test]
    fn a_clock_that_runs_backwards_does_not_panic_or_fire() {
        let (mut timer, start) = timer();
        let later = after(start, 10);
        assert!(!timer.is_due(later, Some(1)));
        assert!(!timer.is_due(start, Some(1)));
    }
}
