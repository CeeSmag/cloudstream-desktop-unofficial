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
fn filter_plugins_sorts_by_name_language_or_size() {
    let hits = catalog();
    let mut query = PluginQuery::default();
    let all = filter_plugins(&hits, &[], &query);
    assert_eq!(
        names(&all),
        ["Anime World", "Animeflv", "Cine Stream", "Zoro"]
    );
    query.sort = PluginSort::Language;
    let by_language = filter_plugins(&hits, &[], &query);
    assert_eq!(
        names(&by_language),
        ["Cine Stream", "Zoro", "Animeflv", "Anime World"]
    );
    query.sort = PluginSort::Size;
    let by_size = filter_plugins(&hits, &[], &query);
    assert_eq!(
        names(&by_size),
        ["Cine Stream", "Animeflv", "Zoro", "Anime World"]
    );
}

#[test]
fn filter_plugins_search_puts_name_matches_first() {
    let mut hits = catalog();
    hits.push(PluginHit {
        repository_name: "Anime Repo".into(),
        ..hit("Kiss", "en", None)
    });
    let query = PluginQuery {
        text: "anime".into(),
        ..PluginQuery::default()
    };
    let found = filter_plugins(&hits, &[], &query);
    assert_eq!(
        names(&found),
        ["Anime World", "Animeflv", "Kiss"],
        "repository match last"
    );
    let query = PluginQuery {
        text: "streams cine".into(),
        ..PluginQuery::default()
    };
    assert_eq!(names(&filter_plugins(&hits, &[], &query)), ["Cine Stream"]);
}

#[test]
fn filter_plugins_language_and_installed_views() {
    let hits = catalog();
    let installed = vec!["Zoro".to_owned()];
    let query = PluginQuery {
        language: "en".into(),
        view: PluginView::NotInstalled,
        ..PluginQuery::default()
    };
    assert_eq!(
        names(&filter_plugins(&hits, &installed, &query)),
        ["Cine Stream"]
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
