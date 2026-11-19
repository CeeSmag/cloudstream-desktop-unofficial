use std::collections::HashMap;

use app_services::PluginHit;
use core_model::PluginListing;

use super::*;

fn hit(name: &str, language: &str, size: Option<u64>) -> PluginHit {
    PluginHit {
        repository_url: "https://repo/repo.json".into(),
        repository_name: "Mega Repo".into(),
        repository_icon_url: None,
        plugin: PluginListing {
            name: name.into(),
            internal_name: name.replace(' ', ""),
            language: Some(language.into()),
            file_size: size,
            description: Some(format!("{name} streams movies")),
            authors: vec!["anon".into()],
            ..PluginListing::default()
        },
    }
}

fn names(list: &[&PluginHit]) -> Vec<String> {
    list.iter().map(|hit| hit.plugin.name.clone()).collect()
}

fn catalog() -> Vec<PluginHit> {
    vec![
        hit("Zoro", "en", Some(200)),
        hit("Anime World", "hi", None),
        hit("Cine Stream", "en", Some(900)),
        hit("Animeflv", "es", Some(500)),
    ]
}

#[test]
fn filter_plugins_sorts_by_name_language_size_or_date_added() {
    let hits = catalog();
    let empty = HashMap::new();
    let mut query = PluginQuery::default();
    let all = filter_plugins(&hits, &empty, &query);
    assert_eq!(
        names(&all),
        ["Anime World", "Animeflv", "Cine Stream", "Zoro"]
    );
    query.sort = PluginSort::Language;
    let by_language = filter_plugins(&hits, &empty, &query);
    assert_eq!(
        names(&by_language),
        ["Cine Stream", "Zoro", "Animeflv", "Anime World"]
    );
    query.sort = PluginSort::Size;
    let by_size = filter_plugins(&hits, &empty, &query);
    assert_eq!(
        names(&by_size),
        ["Cine Stream", "Animeflv", "Zoro", "Anime World"]
    );
    let installed = HashMap::from([
        ("Zoro".to_owned(), 1_700_000_000_000),
        ("Animeflv".to_owned(), 1_750_000_000_000),
        ("AnimeWorld".to_owned(), 0),
    ]);
    query.sort = PluginSort::DateAdded;
    let by_date = filter_plugins(&hits, &installed, &query);
    assert_eq!(
        names(&by_date),
        ["Animeflv", "Zoro", "Anime World", "Cine Stream"]
    );
}

#[test]
fn filter_plugins_search_puts_name_matches_first() {
    let mut hits = catalog();
    let empty = HashMap::new();
    hits.push(PluginHit {
        repository_name: "Anime Repo".into(),
        ..hit("Kiss", "en", None)
    });
    let query = PluginQuery {
        text: "anime".into(),
        ..PluginQuery::default()
    };
    let found = filter_plugins(&hits, &empty, &query);
    assert_eq!(
        names(&found),
        ["Anime World", "Animeflv", "Kiss"],
        "repository match last"
    );
    let query = PluginQuery {
        text: "streams cine".into(),
        ..PluginQuery::default()
    };
    assert_eq!(
        names(&filter_plugins(&hits, &empty, &query)),
        ["Cine Stream"]
    );
}

#[test]
fn filter_plugins_language_repository_and_installed_views() {
    let mut hits = catalog();
    hits.push(PluginHit {
        repository_url: "https://other/repo.json".into(),
        repository_name: "Other Repo".into(),
        ..hit("Other Only", "en", Some(100))
    });
    let installed = HashMap::from([("Zoro".to_owned(), 1_700_000_000_000)]);
    let query = PluginQuery {
        language: "en".into(),
        repository_url: "https://repo/repo.json".into(),
        view: PluginView::NotInstalled,
        ..PluginQuery::default()
    };
    assert_eq!(
        names(&filter_plugins(&hits, &installed, &query)),
        ["Cine Stream"]
    );
    let repo_only = PluginQuery {
        repository_url: "https://other/repo.json".into(),
        ..PluginQuery::default()
    };
    assert_eq!(
        names(&filter_plugins(&hits, &installed, &repo_only)),
        ["Other Only"]
    );
    let query = PluginQuery {
        view: PluginView::Installed,
        ..PluginQuery::default()
    };
    assert_eq!(names(&filter_plugins(&hits, &installed, &query)), ["Zoro"]);
}

#[test]
fn language_choices_lists_each_language_once() {
    assert_eq!(language_choices(&catalog()), ["en", "es", "hi"]);
}

#[test]
fn format_file_size_handles_bytes_kb_and_mb() {
    assert_eq!(format_file_size(None), None);
    assert_eq!(format_file_size(Some(512)), Some("512 B".to_owned()));
    assert_eq!(format_file_size(Some(6_929)), Some("6.8 KB".to_owned()));
    assert_eq!(format_file_size(Some(2_097_152)), Some("2.0 MB".to_owned()));
}
