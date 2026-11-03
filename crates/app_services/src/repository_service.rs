use std::sync::Arc;

use core_model::{DiscoveredRepository, PluginListing, RepositoryEntry};
use extension_repo::{HttpFetcher, RepositoryClient};
use storage::RepositoryStore;

use crate::{Clock, ServiceError};

/// How many repositories are asked for their icon at the same time.
const ICON_WORKERS: usize = 6;

/// Repositories the user subscribed to and the plugins they offer.
#[derive(Clone)]
pub struct RepositoryService {
    client: Arc<RepositoryClient<Arc<dyn HttpFetcher>>>,
    store: RepositoryStore,
    clock: Arc<dyn Clock>,
}

impl RepositoryService {
    /// Creates the service.
    pub fn new(
        fetcher: Arc<dyn HttpFetcher>,
        store: RepositoryStore,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            client: Arc::new(RepositoryClient::new(fetcher)),
            store,
            clock,
        }
    }

    /// Validates a repository URL by downloading its manifest, then remembers it.
    #[tracing::instrument(level = "debug", skip_all, fields(url = ?url), err)]
    pub fn add(&self, url: &str) -> Result<RepositoryEntry, ServiceError> {
        let url = url.trim();
        let manifest = self.client.fetch_manifest(url)?;
        let entry = RepositoryEntry {
            url: url.to_owned(),
            name: manifest.name.clone(),
            added_at_ms: self.clock.now_ms(),
            icon_url: manifest
                .icon_url
                .clone()
                .filter(|icon| !icon.trim().is_empty()),
        };
        self.store.add(&entry)?;
        // The manifest was just read, so a repository without an icon is settled too.
        self.remember_icon(&entry.url, &manifest);
        tracing::info!(url, name = %entry.name, "repository added");
        Ok(entry)
    }

    /// Saves repositories an extension asked for, skipping ones already known and ones without a
    /// URL. Nothing is downloaded: the extension already read each manifest. Returns how many are new.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn import_discovered(&self, found: &[DiscoveredRepository]) -> Result<usize, ServiceError> {
        let mut known: std::collections::HashSet<String> = self
            .store
            .list()?
            .into_iter()
            .map(|entry| entry.url)
            .collect();
        let mut added = 0;
        for repository in found {
            let url = repository.url.trim();
            if url.is_empty() || !known.insert(url.to_owned()) {
                continue;
            }
            let name = if repository.name.trim().is_empty() {
                url
            } else {
                repository.name.trim()
            };
            self.store.add(&RepositoryEntry {
                url: url.to_owned(),
                name: name.to_owned(),
                added_at_ms: self.clock.now_ms(),
                icon_url: repository
                    .icon_url
                    .clone()
                    .filter(|icon| !icon.trim().is_empty()),
            })?;
            added += 1;
        }
        tracing::info!(
            offered = found.len(),
            added,
            "repositories imported from extensions"
        );
        Ok(added)
    }

    /// Forgets a repository (installed plugins stay).
    #[tracing::instrument(level = "debug", skip_all, fields(url = ?url), err)]
    pub fn remove(&self, url: &str) -> Result<(), ServiceError> {
        Ok(self.store.remove(url)?)
    }

    /// Lists the saved repositories.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list(&self) -> Result<Vec<RepositoryEntry>, ServiceError> {
        Ok(self.store.list()?)
    }

    /// Downloads the plugin list of one repository.
    #[tracing::instrument(level = "debug", skip_all, fields(url = ?url), err)]
    pub fn plugins(&self, url: &str) -> Result<Vec<PluginListing>, ServiceError> {
        let manifest = self.client.fetch_manifest(url)?;
        self.remember_icon(url, &manifest);
        Ok(self.client.fetch_plugins(&manifest))
    }

    /// Looks up the icon of every saved repository that has not been looked at yet, a few at a
    /// time. A repository that cannot be reached is tried again next time. Returns how many
    /// repositories were looked at.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn refresh_missing_icons(&self) -> Result<usize, ServiceError> {
        let pending = self.store.urls_without_icon_info()?;
        let next = std::sync::atomic::AtomicUsize::new(0);
        let looked_at = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..ICON_WORKERS.min(pending.len()) {
                scope.spawn(|| loop {
                    let position = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(url) = pending.get(position) else {
                        break;
                    };
                    match self.client.fetch_manifest(url) {
                        Ok(manifest) => {
                            self.remember_icon(url, &manifest);
                            looked_at.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                        Err(error) => tracing::debug!(%url, %error, "could not read the repository to find its icon"),
                    }
                });
            }
        });
        Ok(looked_at.load(std::sync::atomic::Ordering::SeqCst))
    }

    fn remember_icon(&self, url: &str, manifest: &extension_repo::RepositoryManifest) {
        let icon = manifest
            .icon_url
            .as_deref()
            .filter(|icon| !icon.trim().is_empty());
        if let Err(error) = self.store.set_icon(url, icon) {
            tracing::warn!(%url, %error, "could not save the repository icon");
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn plugins_lists_what_the_repository_offers() {
        let plugins = service().plugins("https://r/repo.json").expect("lists");
        assert_eq!(plugins[0].internal_name, "A");
    }
}
