//! The rules for how long saved links are trusted and how much is kept.

use std::time::Duration;

use crate::cached_title::Freshness;

/// The rules of the cache; [`CacheLimits::default`] holds the ones the app uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLimits {
    /// Links saved less long ago than this are played at once.
    pub fresh_for: Duration,
    /// Links saved longer ago than this are forgotten; in between they are tested first.
    pub keep_for: Duration,
    /// How many titles are kept; the least recently saved go first.
    pub max_titles: usize,
    /// How many links are kept per title.
    pub max_links: usize,
    /// The size a title's file may have; a bigger one is refused when saving and deleted when found.
    pub max_file_bytes: u64,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            fresh_for: Duration::from_secs(20 * 60),
            keep_for: Duration::from_secs(24 * 3600),
            max_titles: 50,
            max_links: 200,
            max_file_bytes: 1024 * 1024,
        }
    }
}

/// What the age of a saved title means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Use the links now.
    Usable(Freshness, Option<Duration>),
    /// Too old (or dated absurdly far ahead): delete it.
    Expired,
    /// Saved before the viewer switched provider: never used again, so delete it.
    Invalidated,
}

impl CacheLimits {
    /// Judges a title saved at `saved_at` when it is `now` (both in seconds since 1970). A title
    /// saved in the future (the clock was set back) is never fresh, and one dated more than
    /// [`CacheLimits::keep_for`] ahead is as good as expired.
    pub(crate) fn judge(&self, saved_at: u64, now: u64) -> Verdict {
        if saved_at > now {
            let ahead = Duration::from_secs(saved_at - now);
            return if ahead > self.keep_for {
                Verdict::Expired
            } else {
                Verdict::Usable(Freshness::Stale, None)
            };
        }
        let age = Duration::from_secs(now - saved_at);
        if age < self.fresh_for {
            Verdict::Usable(Freshness::Fresh, Some(age))
        } else if age <= self.keep_for {
            Verdict::Usable(Freshness::Stale, Some(age))
        } else {
            Verdict::Expired
        }
    }
}

#[cfg(test)]
#[path = "cache_limits_tests.rs"]
mod tests;
