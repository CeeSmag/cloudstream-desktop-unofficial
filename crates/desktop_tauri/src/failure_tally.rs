//! Counts why streams failed their check, so each video ends with one line that says what went
//! wrong (which kinds of failure, on which hosts) instead of dozens of separate ones.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

/// How many hosts the summary names.
const TOP_HOSTS: usize = 8;

#[derive(Default)]
struct Counts {
    total: usize,
    reasons: BTreeMap<String, usize>,
    hosts: BTreeMap<String, usize>,
}

/// The failed checks of one video, counted by reason and by host.
#[derive(Default)]
pub struct FailureTally {
    counts: Mutex<Counts>,
}

/// The reason without the addresses in it, so equal failures on different links count together:
/// `cannot download https://a.test/x?sig=1: timed out` becomes `cannot download <url> timed out`.
pub fn reason_kind(reason: &str) -> String {
    reason
        .split_whitespace()
        .map(|word| {
            if word.starts_with("http") {
                "<url>"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn most_frequent_first(counts: &BTreeMap<String, usize>, limit: usize) -> String {
    let mut ordered: Vec<(&String, &usize)> = counts.iter().collect();
    ordered.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    ordered
        .into_iter()
        .take(limit)
        .map(|(name, count)| format!("{name}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl FailureTally {
    /// Counts one failed check of a stream on `host`.
    pub fn record(&self, host: &str, reason: &str) {
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        counts.total += 1;
        *counts.reasons.entry(reason_kind(reason)).or_default() += 1;
        *counts.hosts.entry(host.to_owned()).or_default() += 1;
    }

    /// How many checks failed.
    pub fn total(&self) -> usize {
        self.counts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .total
    }

    /// The failure reasons with their counts, most frequent first.
    pub fn reasons(&self) -> String {
        let counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        most_frequent_first(&counts.reasons, usize::MAX)
    }

    /// The hosts with the most failures, most frequent first.
    pub fn hosts(&self) -> String {
        let counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        most_frequent_first(&counts.hosts, TOP_HOSTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_left_out_of_the_reason_so_equal_failures_count_together() {
        assert_eq!(
            reason_kind("cannot download https://a.test/x?sig=1: timed out"),
            "cannot download <url> timed out"
        );
        assert_eq!(reason_kind("HTTP 403"), "HTTP 403");
    }

    #[test]
    fn failures_are_counted_by_reason_and_by_host_most_frequent_first() {
        let tally = FailureTally::default();
        tally.record("a.test", "HTTP 403");
        tally.record("b.test", "HTTP 403");
        tally.record("a.test", "a web page, not a video");
        assert_eq!(tally.total(), 3);
        assert_eq!(tally.reasons(), "HTTP 403=2, a web page, not a video=1");
        assert_eq!(tally.hosts(), "a.test=2, b.test=1");
    }

    #[test]
    fn nothing_failed_gives_empty_summaries() {
        let tally = FailureTally::default();
        assert_eq!(
            (tally.total(), tally.reasons(), tally.hosts()),
            (0, String::new(), String::new())
        );
    }
}
