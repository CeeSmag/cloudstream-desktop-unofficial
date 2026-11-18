//! How long a plugin call may keep waiting while its requests are being answered in the browser.
//!
//! Some plugins make many requests for one call (AnimePahe loads a long show's episode list one page per
//! second, so One Piece needs far more than the 60 s a call normally gets). While the browser keeps
//! answering such requests the call is making progress, so its deadline is pushed ahead; when requests
//! stop, or the call has already taken `MAX_BURST`, the call's own timeout applies again.

use std::time::{Duration, Instant};

/// A pause in requests this long starts a new burst.
const QUIET_RESETS: Duration = Duration::from_secs(60);
/// No call is held longer than this in one burst, so a plugin stuck in a loop still times out.
pub const MAX_BURST: Duration = Duration::from_secs(10 * 60);

/// Decides whether the calls waiting on the browser may be given more time.
#[derive(Debug, Default)]
pub struct Patience {
    burst_start: Option<Instant>,
    last_request: Option<Instant>,
    requests: u32,
    limited: u32,
}

/// What a burst of requests has done so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Requests answered by the browser in this burst.
    pub requests: u32,
    /// Of those, how many the site refused as too many (HTTP 429).
    pub limited: u32,
    /// Seconds since the burst began.
    pub elapsed_s: u64,
}

impl Patience {
    /// A request is being answered at `now`: whether waiting calls get more time.
    pub fn allow(&mut self, now: Instant) -> bool {
        let quiet = self
            .last_request
            .is_none_or(|last| now.saturating_duration_since(last) >= QUIET_RESETS);
        if quiet {
            self.burst_start = Some(now);
            self.requests = 0;
            self.limited = 0;
        }
        self.last_request = Some(now);
        self.burst_start
            .is_some_and(|start| now.saturating_duration_since(start) < MAX_BURST)
    }
}

impl Patience {
    /// An answered request with this HTTP status (None when the browser could not make it).
    pub fn record(&mut self, status: Option<u16>) {
        self.requests += 1;
        if status == Some(429) {
            self.limited += 1;
        }
    }

    /// The burst so far, as of `now`.
    pub fn progress(&self, now: Instant) -> Progress {
        let elapsed = self
            .burst_start
            .map_or(0, |start| now.saturating_duration_since(start).as_secs());
        Progress {
            requests: self.requests,
            limited: self.limited,
            elapsed_s: elapsed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_requests_keep_the_call_waiting() {
        let (mut patience, start) = (Patience::default(), Instant::now());
        for second in 0..120 {
            assert!(patience.allow(start + Duration::from_secs(second)));
        }
    }

    #[test]
    fn a_burst_that_never_ends_is_cut_off() {
        let (mut patience, start) = (Patience::default(), Instant::now());
        let mut allowed_until = 0;
        for second in 0..(MAX_BURST.as_secs() + 30) {
            if patience.allow(start + Duration::from_secs(second)) {
                allowed_until = second;
            }
        }
        assert_eq!(allowed_until, MAX_BURST.as_secs() - 1);
    }

    #[test]
    fn a_quiet_minute_starts_a_new_burst() {
        let (mut patience, start) = (Patience::default(), Instant::now());
        assert!(patience.allow(start));
        let later = start + MAX_BURST + QUIET_RESETS;
        assert!(
            patience.allow(later),
            "after a quiet minute the call is held again"
        );
    }

    #[test]
    fn progress_counts_requests_and_refusals_and_a_new_burst_starts_over() {
        let (mut patience, start) = (Patience::default(), Instant::now());
        for (second, status) in [(0, 200), (1, 200), (2, 429)] {
            patience.allow(start + Duration::from_secs(second));
            patience.record(Some(status));
        }
        let now = start + Duration::from_secs(3);
        assert_eq!(
            patience.progress(now),
            Progress {
                requests: 3,
                limited: 1,
                elapsed_s: 3
            }
        );
        let later = start + Duration::from_secs(300);
        patience.allow(later);
        assert_eq!(patience.progress(later).requests, 0);
    }
}
