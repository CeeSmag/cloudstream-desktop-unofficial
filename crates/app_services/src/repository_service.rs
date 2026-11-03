use std::sync::{Arc, Mutex, PoisonError};

use core_model::{DiscoveredRepository, PluginListing, RepositoryEntry};
use extension_repo::{HttpFetcher, RepositoryClient};
use storage::RepositoryStore;

use crate::plugin_index::{IndexedRepository, PluginHit, PluginIndex};
use crate::{Clock, ServiceError};

/// How many repositories are asked for their icon at the same time.
const ICON_WORKERS: usize = 6;

/// Repositories the user subscribed to and the plugins they offer.
#[derive(Clone)]
pub struct RepositoryService {
    client: Arc<RepositoryClient<Arc<dyn HttpFetcher>>>,
    store: RepositoryStore,
    clock: Arc<dyn Clock>,
    /// Every extension of every repository, once it has been downloaded.
    index: Arc<Mutex<Option<Arc<PluginIndex>>>>,
    /// Held while the index is being built, so two searches do not both download everything.
    building: Arc<Mutex<()>>,
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
            index: Arc::default(),
            building: Arc::default(),
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
        self.forget_index();
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
        if added > 0 {
            self.forget_index();
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
        self.store.remove(url)?;
        self.forget_index();
        Ok(())
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

    /// Downloads the extensions of every saved repository (a few repositories at a time) so they
    /// can be listed. Does nothing when that was already done. Returns how many extensions
    /// there are. A repository that cannot be reached is left out until the next time.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn prepare_plugin_catalog(&self) -> Result<usize, ServiceError> {
        Ok(self.plugin_index()?.plugin_count())
    }

    /// Every extension of every repository, each with the repository that offers it. The first
    /// call downloads them all; later ones answer at once.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn all_plugins(&self) -> Result<Vec<PluginHit>, ServiceError> {
        Ok(self.plugin_index()?.all())
    }

    fn plugin_index(&self) -> Result<Arc<PluginIndex>, ServiceError> {
        let _building = self.building.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = self.current_index() {
            return Ok(index);
        }
        let started = std::time::Instant::now();
        let index = Arc::new(PluginIndex::new(self.download_everything()?));
        tracing::info!(
            plugins = index.plugin_count(),
            elapsed_ms = started.elapsed().as_millis(),
            "extension list built"
        );
        *self.index.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&index));
        Ok(index)
    }

    fn current_index(&self) -> Option<Arc<PluginIndex>> {
        self.index
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn forget_index(&self) {
        *self.index.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn download_everything(&self) -> Result<Vec<IndexedRepository>, ServiceError> {
        let repositories = self.store.list()?;
        let next = std::sync::atomic::AtomicUsize::new(0);
        let found: Mutex<Vec<(usize, IndexedRepository)>> = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..ICON_WORKERS.min(repositories.len()) {
                scope.spawn(|| loop {
                    let position = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(entry) = repositories.get(position) else {
                        break;
                    };
                    match self.client.fetch_manifest(&entry.url) {
                        Ok(manifest) => {
                            self.remember_icon(&entry.url, &manifest);
                            let indexed = IndexedRepository {
                                url: entry.url.clone(),
                                name: entry.name.clone(),
                                icon_url: manifest
                                    .icon_url
                                    .clone()
                                    .filter(|icon| !icon.trim().is_empty())
                                    .or_else(|| entry.icon_url.clone()),
                                plugins: self.client.fetch_plugins(&manifest),
                            };
                            found
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .push((position, indexed));
                        }
                        Err(error) => tracing::debug!(url = %entry.url, %error, "repository left out of the extension list"),
                    }
                });
            }
        });
        let mut found = found.into_inner().unwrap_or_else(PoisonError::into_inner);
        found.sort_by_key(|(position, _)| *position);
        Ok(found
            .into_iter()
            .map(|(_, repository)| repository)
            .collect())
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
#[path = "repository_service_tests.rs"]
mod tests;
