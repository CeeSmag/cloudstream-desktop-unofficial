//! The Extensions > Repositories list: search, sort (Date added or Name), host & date formatting.

use std::cmp::{Ordering, Reverse};

use core_model::RepositoryEntry;

/// How the Repositories list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RepositorySort {
    /// Most recently added first; unknown (`0`) last.
    #[default]
    DateAdded,
    /// Alphabetical by display name (A–Z).
    Name,
}

/// What the viewer narrowed and sorted the Repositories list to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RepositoryQuery {
    pub(crate) text: String,
    pub(crate) sort: RepositorySort,
}

/// Repositories matching `query`, ordered by `query.sort`.
pub(crate) fn filter_repositories<'a>(
    repositories: &'a [RepositoryEntry],
    query: &RepositoryQuery,
) -> Vec<&'a RepositoryEntry> {
    let whole = query.text.trim().to_lowercase();
    let words: Vec<&str> = whole.split_whitespace().collect();
    let mut kept: Vec<&RepositoryEntry> = repositories
        .iter()
        .filter(|repo| matches_words(repo, &words))
        .collect();
    kept.sort_by(|a, b| compare(a, b, query.sort));
    kept
}

/// Extracts the host from a URL (`https://raw.githubusercontent.com/…` → `raw.githubusercontent.com`).
pub(crate) fn repository_host(url: &str) -> String {
    let trimmed = url.trim();
    let rest = trimmed
        .split_once("://")
        .map(|(_, after)| after)
        .unwrap_or(trimmed);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(trimmed);
    let host = authority
        .rsplit_once('@')
        .map(|(_, after)| after)
        .unwrap_or(authority);
    if host.is_empty() {
        trimmed.to_owned()
    } else {
        host.to_owned()
    }
}

/// Formats a positive Unix timestamp in milliseconds as `YYYY-MM-DD` (UTC). Returns `None` for `<= 0`.
pub(crate) fn format_added_date(added_at_ms: i64) -> Option<String> {
    if added_at_ms <= 0 {
        return None;
    }
    let days = added_at_ms.div_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Builds the subtitle shown beneath a repository name: host and, when known, its added date.
pub(crate) fn repository_detail(repo: &RepositoryEntry) -> String {
    let host = repository_host(&repo.url);
    match format_added_date(repo.added_at_ms) {
        Some(date) => format!("{host} · Added {date}"),
        None => host,
    }
}

fn matches_words(repo: &RepositoryEntry, words: &[&str]) -> bool {
    if words.is_empty() {
        return true;
    }
    let haystack = format!("{} {}", repo.name, repo.url).to_lowercase();
    words.iter().all(|word| haystack.contains(word))
}

fn compare(a: &RepositoryEntry, b: &RepositoryEntry, sort: RepositorySort) -> Ordering {
    let by_name = || {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.url.cmp(&b.url))
    };
    match sort {
        RepositorySort::Name => by_name(),
        RepositorySort::DateAdded => date_key(a.added_at_ms)
            .cmp(&date_key(b.added_at_ms))
            .then_with(by_name),
    }
}

fn date_key(added_at_ms: i64) -> (u8, Reverse<i64>) {
    if added_at_ms > 0 {
        (0, Reverse(added_at_ms))
    } else {
        (1, Reverse(0))
    }
}

/// Howard Hinnant's civil-from-days algorithm (days since 1970-01-01 → proleptic Gregorian UTC date).
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(m <= 2);
    (
        year,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

#[cfg(test)]
#[path = "repository_filter_tests.rs"]
mod tests;
