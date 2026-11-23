use super::*;
use std::collections::HashSet;

fn reviewed(url: &str, name: &str) -> Option<&'static ReviewedPlugin> {
    find_reviewed(REVIEWED, url, name)
}

#[test]
fn every_entry_has_a_reason_and_a_normalised_repository() {
    for item in REVIEWED {
        assert!(
            !item.reason.trim().is_empty(),
            "{} needs a reason",
            item.internal_name
        );
        assert!(!item.internal_name.trim().is_empty());
        // Stored as RepositoryId gives it, or it would never match.
        assert_eq!(
            RepositoryId::of(item.repository).as_str(),
            item.repository,
            "{item:?}"
        );
    }
}

#[test]
fn no_plugin_is_listed_twice() {
    let mut seen = HashSet::new();
    for item in REVIEWED {
        assert!(
            seen.insert((item.repository, item.internal_name)),
            "duplicate {item:?}"
        );
    }
}

#[test]
fn entries_match_any_spelling_of_their_repository() {
    for url in [
        "https://raw.githubusercontent.com/doGior/doGiorsHadEnough/refs/heads/builds/repo.json",
        "https://raw.githubusercontent.com/doGior/doGiorsHadEnough/builds/repo.json",
    ] {
        assert!(reviewed(url, "YouTube").is_some(), "{url}");
    }
}

#[test]
fn a_same_named_plugin_in_another_repository_is_not_reviewed() {
    let elsewhere = "https://raw.githubusercontent.com/someone/games/builds/repo.json";
    assert!(reviewed(elsewhere, "YouTube").is_none());
    assert!(reviewed(elsewhere, "MegaProvider").is_none());
    // Internal names are matched exactly.
    let dogior = "https://raw.githubusercontent.com/doGior/doGiorsHadEnough/builds/repo.json";
    assert!(reviewed(dogior, "youtube").is_none());
}

#[test]
fn megaprovider_is_the_only_non_content_entry() {
    let non_content: Vec<_> = REVIEWED
        .iter()
        .filter(|item| item.reason.starts_with("No content"))
        .map(|item| item.internal_name)
        .collect();
    assert_eq!(non_content, vec!["MegaProvider"]);
}
