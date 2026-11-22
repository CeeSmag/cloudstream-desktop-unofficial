//! Path / provider / link-search routing tables for [`crate::PluginWorkerPool`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use extension_api::{GatewayError, ProviderGateway};

/// One loaded plugin's gateway (its own JVM process in production).
pub(crate) type Worker = Arc<dyn ProviderGateway>;

/// Metadata kept after a worker is parked so it can be revived without rediscovery.
#[derive(Debug, Clone)]
pub(crate) struct PluginRecord {
    pub(crate) trusted: bool,
    pub(crate) providers: Vec<String>,
}

/// Maps that route provider and link-search ids to the worker that owns them.
#[derive(Default)]
pub(crate) struct WorkerMaps {
    by_path: HashMap<PathBuf, Worker>,
    provider_to_path: HashMap<String, PathBuf>,
    search_to_path: HashMap<String, PathBuf>,
    /// Every loaded plugin (live or parked).
    records: HashMap<PathBuf, PluginRecord>,
    /// Live workers oldest-first for LRU eviction.
    live_order: Vec<PathBuf>,
    /// Calls in progress per plugin; a plugin with any is never parked (see `WorkerLease`).
    in_flight: HashMap<PathBuf, usize>,
}

impl WorkerMaps {
    /// Registers a freshly loaded worker and the providers it reported.
    pub(crate) fn insert(&mut self, path: PathBuf, worker: Worker, record: PluginRecord) {
        self.clear_path_routes(&path);
        for name in &record.providers {
            self.provider_to_path.insert(name.clone(), path.clone());
        }
        self.records.insert(path.clone(), record);
        self.touch_live(&path);
        self.by_path.insert(path, worker);
    }

    /// Like [`Self::insert`], and counts one call in progress, in the same step: nobody can park
    /// the worker between "it is live" and "its first caller is protected".
    pub(crate) fn insert_leased(&mut self, path: PathBuf, worker: Worker, record: PluginRecord) {
        self.insert(path.clone(), worker, record);
        *self.in_flight.entry(path).or_default() += 1;
    }

    /// The live worker of `provider` with one call in progress counted, or `None` when it is
    /// parked. Looking it up and protecting it are one step.
    pub(crate) fn lease_provider(
        &mut self,
        provider: &str,
    ) -> Result<Option<(Worker, PathBuf)>, GatewayError> {
        let path = self.path_for_provider(provider)?;
        let Some(worker) = self.by_path.get(&path).cloned() else {
            return Ok(None);
        };
        self.touch_live(&path);
        *self.in_flight.entry(path.clone()).or_default() += 1;
        Ok(Some((worker, path)))
    }

    /// A call on `path` ended.
    pub(crate) fn release(&mut self, path: &Path) {
        if let Some(count) = self.in_flight.get_mut(path) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.in_flight.remove(path);
            }
        }
    }

    fn is_leased(&self, path: &Path) -> bool {
        self.in_flight.get(path).is_some_and(|count| *count > 0)
    }

    /// Parks the oldest live plugin that is not `keep`, has no call in progress and no open link
    /// search, choosing and parking in one step: a call that starts a moment later cannot find its
    /// worker already chosen. Returns it with its path.
    pub(crate) fn park_oldest_idle(&mut self, keep: &Path) -> Option<(PathBuf, Worker)> {
        let path = self.oldest_live_except(keep)?;
        let worker = self.park(&path)?;
        Some((path, worker))
    }

    /// Drops the live worker but keeps provider routes and the record (for later revive).
    pub(crate) fn park(&mut self, path: &Path) -> Option<Worker> {
        // Always drop LRU / search routes even when already parked, or enforce loops forever.
        self.live_order.retain(|entry| entry != path);
        self.search_to_path.retain(|_, mapped| mapped != path);
        self.by_path.remove(path)
    }

    /// Drops the worker for `path` and every route that pointed at it.
    pub(crate) fn remove(&mut self, path: &Path) -> Option<Worker> {
        let worker = self.by_path.remove(path);
        self.live_order.retain(|entry| entry != path);
        self.records.remove(path);
        self.clear_path_routes(path);
        worker
    }

    pub(crate) fn record(&self, path: &Path) -> Option<&PluginRecord> {
        self.records.get(path)
    }

    pub(crate) fn path_for_provider(&self, provider: &str) -> Result<PathBuf, GatewayError> {
        self.provider_to_path.get(provider).cloned().ok_or_else(|| {
            GatewayError::Unavailable(format!("no worker loaded for provider '{provider}'"))
        })
    }

    pub(crate) fn live_count(&self) -> usize {
        self.by_path.len()
    }

    pub(crate) fn by_path_contains(&self, path: &Path) -> bool {
        self.by_path.contains_key(path)
    }

    /// Oldest live path that is not `keep`, has no call in progress and no open link search.
    pub(crate) fn oldest_live_except(&self, keep: &Path) -> Option<PathBuf> {
        self.live_order
            .iter()
            .find(|path| {
                path.as_path() != keep && !self.is_leased(path) && !self.has_open_search(path)
            })
            .cloned()
    }

    fn has_open_search(&self, path: &Path) -> bool {
        self.search_to_path
            .values()
            .any(|mapped| mapped.as_path() == path)
    }

    /// Marks `path` as the most recently used live worker (LRU).
    pub(crate) fn touch_live(&mut self, path: &Path) {
        self.live_order.retain(|entry| entry != path);
        self.live_order.push(path.to_path_buf());
    }

    /// Takes every worker and clears all routes (app shutdown).
    pub(crate) fn take_all(&mut self) -> Vec<Worker> {
        let workers: Vec<Worker> = self.by_path.drain().map(|(_, worker)| worker).collect();
        self.provider_to_path.clear();
        self.search_to_path.clear();
        self.records.clear();
        self.live_order.clear();
        self.in_flight.clear();
        workers
    }

    /// Every live worker in path order (stable fan-out for `extract` and broadcasts).
    pub(crate) fn all_workers(&self) -> Vec<Worker> {
        let mut entries: Vec<(&PathBuf, &Worker)> = self.by_path.iter().collect();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        entries
            .into_iter()
            .map(|(_, worker)| Arc::clone(worker))
            .collect()
    }

    pub(crate) fn worker_for_search(&self, id: &str) -> Result<Worker, GatewayError> {
        let path = self.search_to_path.get(id).ok_or_else(|| {
            GatewayError::Unavailable(format!("no worker owns link search '{id}'"))
        })?;
        self.by_path.get(path).cloned().ok_or_else(|| {
            GatewayError::Unavailable(format!("worker for link search '{id}' is gone"))
        })
    }

    /// Remembers which worker started search `id`.
    pub(crate) fn remember_search(&mut self, id: String, path: PathBuf) {
        self.search_to_path.insert(id, path);
    }

    pub(crate) fn forget_search(&mut self, id: &str) {
        self.search_to_path.remove(id);
    }

    fn clear_path_routes(&mut self, path: &Path) {
        self.provider_to_path.retain(|_, mapped| mapped != path);
        self.search_to_path.retain(|_, mapped| mapped != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use core_model::{
        HomePage, LoadDetails, MainPageEntry, PlayableLinks, ProviderInfo, SearchPage,
    };

    struct EmptyWorker;

    impl ProviderGateway for EmptyWorker {
        fn load_extension(&self, _: &Path, _: bool) -> Result<Vec<ProviderInfo>, GatewayError> {
            Ok(Vec::new())
        }
        fn search(&self, _: &str, _: &str, _: u32) -> Result<SearchPage, GatewayError> {
            Ok(SearchPage::default())
        }
        fn main_page(
            &self,
            _: &str,
            _: u32,
            _: &MainPageEntry,
        ) -> Result<Option<HomePage>, GatewayError> {
            Ok(None)
        }
        fn load(&self, _: &str, _: &str) -> Result<Option<LoadDetails>, GatewayError> {
            Ok(None)
        }
        fn load_links(&self, _: &str, _: &str) -> Result<PlayableLinks, GatewayError> {
            Ok(PlayableLinks::default())
        }
        fn extract(&self, _: &str, _: Option<&str>) -> Result<PlayableLinks, GatewayError> {
            Ok(PlayableLinks::default())
        }
    }

    #[test]
    fn insert_routes_providers_and_remove_clears_them() {
        let path = PathBuf::from("/ext/A.cs3");
        let mut maps = WorkerMaps::default();
        maps.insert(
            path.clone(),
            Arc::new(EmptyWorker),
            PluginRecord {
                trusted: false,
                providers: vec!["Alpha".to_owned()],
            },
        );
        assert!(matches!(maps.lease_provider("Alpha"), Ok(Some(_))));
        assert!(maps.remove(&path).is_some());
        assert!(
            maps.lease_provider("Alpha").is_err(),
            "the route went with the plugin"
        );
        assert!(maps.all_workers().is_empty());
    }

    #[test]
    fn park_keeps_provider_route_without_a_live_worker() {
        let path = PathBuf::from("/ext/A.cs3");
        let mut maps = WorkerMaps::default();
        maps.insert(
            path.clone(),
            Arc::new(EmptyWorker),
            PluginRecord {
                trusted: true,
                providers: vec!["Alpha".to_owned()],
            },
        );
        assert!(maps.park(&path).is_some());
        assert!(maps.by_path.is_empty());
        assert_eq!(maps.path_for_provider("Alpha").expect("path"), path);
        assert!(maps.record(&path).is_some_and(|r| r.trusted));
        assert!(
            matches!(maps.lease_provider("Alpha"), Ok(None)),
            "parked: the route stays, there is no worker to lease"
        );
    }

    #[test]
    fn a_leased_plugin_is_never_the_one_chosen_to_park() {
        let (old, new) = (PathBuf::from("/ext/old.cs3"), PathBuf::from("/ext/new.cs3"));
        let mut maps = WorkerMaps::default();
        for (path, name) in [(&old, "Old"), (&new, "New")] {
            let record = PluginRecord {
                trusted: false,
                providers: vec![name.to_owned()],
            };
            maps.insert(path.clone(), Arc::new(EmptyWorker), record);
        }
        assert!(matches!(maps.lease_provider("Old"), Ok(Some(_))));
        // Leasing counts as use, so make the leased plugin the oldest again to test the skip.
        maps.touch_live(&new);
        assert_eq!(
            maps.oldest_live_except(Path::new("")),
            Some(new.clone()),
            "the oldest plugin has a call in progress, so the next one is chosen"
        );
        maps.release(&old);
        assert_eq!(maps.oldest_live_except(Path::new("")), Some(old));
    }

    #[test]
    fn remember_search_routes_poll_to_the_provider_worker() {
        let path = PathBuf::from("/ext/B.cs3");
        let mut maps = WorkerMaps::default();
        maps.insert(
            path.clone(),
            Arc::new(EmptyWorker),
            PluginRecord {
                trusted: false,
                providers: vec!["Beta".to_owned()],
            },
        );
        maps.remember_search("s1".to_owned(), path);
        assert!(maps.worker_for_search("s1").is_ok());
        maps.forget_search("s1");
        assert!(maps.worker_for_search("s1").is_err());
    }
}
