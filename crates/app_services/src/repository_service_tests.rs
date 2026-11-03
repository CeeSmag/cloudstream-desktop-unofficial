use std::collections::HashMap;

use extension_repo::RepoError;
use storage::Database;

use super::*;
use crate::test_support::FixedClock;

struct MapFetcher(HashMap<String, Vec<u8>>);

impl HttpFetcher for MapFetcher {
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RepoError> {
        self.0.get(url).cloned().ok_or(RepoError::Download {
            url: url.into(),
            reason: "404".into(),
        })
    }
}

fn service() -> RepositoryService {
    let files = HashMap::from([
        (
            "https://r/repo.json".to_owned(),
            br#"{"name":"R","pluginLists":["https://r/plugins.json"]}"#.to_vec(),
        ),
        (
            "https://r/plugins.json".to_owned(),
            br#"[{"internalName":"A","url":"https://r/A.cs3"}]"#.to_vec(),
        ),
    ]);
    let store = RepositoryStore::new(Arc::new(Database::open_in_memory().expect("memory db")));
    RepositoryService::new(
        Arc::new(MapFetcher(files)),
        store,
        Arc::new(FixedClock::at(42)),
    )
}

#[test]
fn add_validates_stores_and_stamps_the_time() {
    let service = service();
    let entry = service.add("  https://r/repo.json ").expect("adds");
    assert_eq!((entry.name.as_str(), entry.added_at_ms), ("R", 42));
    assert_eq!(service.list().expect("list").len(), 1);
}

#[test]
fn add_rejects_an_unreachable_repository_and_stores_nothing() {
    let service = service();
    assert!(service.add("https://nope/repo.json").is_err());
    assert!(service.list().expect("list").is_empty());
}

fn discovered(name: &str, url: &str) -> DiscoveredRepository {
    DiscoveredRepository {
        name: name.into(),
        url: url.into(),
        icon_url: None,
    }
}

#[test]
fn import_discovered_adds_new_repositories_without_downloading_anything() {
    let service = service();
    service.add("https://r/repo.json").expect("adds");
    let added = service
        .import_discovered(&[
            discovered("Known", "https://r/repo.json"),
            discovered("New", "https://n/repo.json"),
            discovered("", "https://unnamed/repo.json"),
            discovered("No url", "  "),
            discovered("Again", "https://n/repo.json"),
        ])
        .expect("imports");
    assert_eq!(added, 2);
    let names: Vec<String> = service
        .list()
        .expect("list")
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["R", "New", "https://unnamed/repo.json"]);
}

#[test]
fn the_icon_of_a_repository_is_saved_when_it_is_added_and_looked_up_for_old_ones() {
    let files = HashMap::from([
        (
            "https://i/repo.json".to_owned(),
            br#"{"name":"I","iconUrl":"https://i/icon.png","pluginLists":[]}"#.to_vec(),
        ),
        (
            "https://n/repo.json".to_owned(),
            br#"{"name":"N","pluginLists":[]}"#.to_vec(),
        ),
    ]);
    let store = RepositoryStore::new(Arc::new(Database::open_in_memory().expect("memory db")));
    let service = RepositoryService::new(
        Arc::new(MapFetcher(files)),
        store,
        Arc::new(FixedClock::at(1)),
    );
    let added = service.add("https://i/repo.json").expect("adds");
    assert_eq!(added.icon_url.as_deref(), Some("https://i/icon.png"));

    // Repositories found by an extension have no icon looked up yet.
    service
        .import_discovered(&[
            discovered("N", "https://n/repo.json"),
            discovered("Gone", "https://gone/repo.json"),
        ])
        .expect("imports");
    let looked_at = service.refresh_missing_icons().expect("refreshes");
    assert_eq!(looked_at, 1, "the unreachable one is left for next time");
    let icons: Vec<Option<String>> = service
        .list()
        .expect("list")
        .into_iter()
        .map(|e| e.icon_url)
        .collect();
    assert_eq!(icons, [Some("https://i/icon.png".to_owned()), None, None]);
    assert_eq!(
        service.refresh_missing_icons().expect("again"),
        0,
        "nothing left to look up but the unreachable one"
    );
}

#[test]
fn the_catalog_lists_extensions_of_all_repositories_and_follows_added_repositories() {
    let files = HashMap::from([
        (
            "https://a/repo.json".to_owned(),
            br#"{"name":"A","pluginLists":["https://a/plugins.json"]}"#.to_vec(),
        ),
        (
            "https://a/plugins.json".to_owned(),
            br#"[{"internalName":"MovieHub","name":"Movie Hub","url":"u"}]"#.to_vec(),
        ),
        (
            "https://b/repo.json".to_owned(),
            br#"{"name":"B","pluginLists":["https://b/plugins.json"]}"#.to_vec(),
        ),
        (
            "https://b/plugins.json".to_owned(),
            br#"[{"internalName":"AnimeLand","name":"Anime Land","url":"u"}]"#.to_vec(),
        ),
    ]);
    let store = RepositoryStore::new(Arc::new(Database::open_in_memory().expect("memory db")));
    let service = RepositoryService::new(
        Arc::new(MapFetcher(files)),
        store,
        Arc::new(FixedClock::at(1)),
    );
    service.add("https://a/repo.json").expect("adds");

    assert_eq!(service.prepare_plugin_catalog().expect("prepares"), 1);
    assert_eq!(service.all_plugins().expect("lists").len(), 1);

    service.add("https://b/repo.json").expect("adds");
    let hits = service.all_plugins().expect("lists");
    let names: Vec<&str> = hits.iter().map(|hit| hit.plugin.name.as_str()).collect();
    assert_eq!(
        names,
        ["Movie Hub", "Anime Land"],
        "the new repository is listed at once"
    );
    assert_eq!(hits[1].repository_name, "B");

    service.remove("https://b/repo.json").expect("removes");
    assert_eq!(service.all_plugins().expect("lists").len(), 1);
}

#[test]
fn plugins_lists_what_the_repository_offers() {
    let plugins = service().plugins("https://r/repo.json").expect("lists");
    assert_eq!(plugins[0].internal_name, "A");
}
