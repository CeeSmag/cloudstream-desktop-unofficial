use core_model::RepositoryEntry;

use super::*;

fn repo(name: &str, url: &str, added_at_ms: i64) -> RepositoryEntry {
    RepositoryEntry {
        url: url.into(),
        name: name.into(),
        added_at_ms,
        icon_url: None,
    }
}

fn names(list: &[&RepositoryEntry]) -> Vec<String> {
    list.iter().map(|entry| entry.name.clone()).collect()
}

fn sample_repos() -> Vec<RepositoryEntry> {
    vec![
        repo(
            "Zeta Repo",
            "https://zeta.example/repo.json",
            1_700_000_000_000,
        ),
        repo(
            "Alpha Hub",
            "https://alpha.example/repo.json",
            1_750_000_000_000,
        ),
        repo(
            "beta streams",
            "https://raw.githubusercontent.com/beta/repo.json",
            0,
        ),
    ]
}

#[test]
fn filter_repositories_sorts_by_date_added_newest_first_with_unknown_last() {
    let repos = sample_repos();
    let query = RepositoryQuery {
        text: String::new(),
        sort: RepositorySort::DateAdded,
    };
    let found = filter_repositories(&repos, &query);
    assert_eq!(names(&found), ["Alpha Hub", "Zeta Repo", "beta streams"]);
}

#[test]
fn filter_repositories_sorts_by_name_case_insensitively() {
    let repos = sample_repos();
    let query = RepositoryQuery {
        text: String::new(),
        sort: RepositorySort::Name,
    };
    let found = filter_repositories(&repos, &query);
    assert_eq!(names(&found), ["Alpha Hub", "beta streams", "Zeta Repo"]);
}

#[test]
fn filter_repositories_searches_name_and_url_with_multiple_words() {
    let repos = sample_repos();
    let by_host = RepositoryQuery {
        text: "github beta".into(),
        sort: RepositorySort::DateAdded,
    };
    assert_eq!(
        names(&filter_repositories(&repos, &by_host)),
        ["beta streams"]
    );
    let by_name = RepositoryQuery {
        text: "ALPHA".into(),
        sort: RepositorySort::DateAdded,
    };
    assert_eq!(names(&filter_repositories(&repos, &by_name)), ["Alpha Hub"]);
}

#[test]
fn repository_host_extracts_domain_or_falls_back() {
    assert_eq!(
        repository_host("https://raw.githubusercontent.com/org/repo/main/repo.json"),
        "raw.githubusercontent.com"
    );
    assert_eq!(repository_host("shortcode"), "shortcode");
}

#[test]
fn format_added_date_formats_known_utc_dates_and_ignores_non_positive() {
    assert_eq!(format_added_date(0), None);
    assert_eq!(format_added_date(-500), None);
    // 2024-02-29T12:00:00Z (leap day)
    assert_eq!(
        format_added_date(1_709_208_000_000),
        Some("2024-02-29".to_owned())
    );
    // 2026-11-19T00:00:00Z
    assert_eq!(
        format_added_date(1_795_046_400_000),
        Some("2026-11-19".to_owned())
    );
}
