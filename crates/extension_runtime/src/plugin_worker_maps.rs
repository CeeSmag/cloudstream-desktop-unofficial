//! Path / provider / link-search routing tables for [`crate::PluginWorkerPool`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use extension_api::{GatewayError, ProviderGateway};

/// One loaded plugin's gateway (its own JVM process in production).
pub(crate) type Worker = Arc<dyn ProviderGateway>;

/// Maps that route provider and link-search ids to the worker that owns them.
#[derive(Default)]
pub(crate) struct WorkerMaps {
    by_path: HashMap<PathBuf, Worker>,
    provider_to_path: HashMap<String, PathBuf>,
    search_to_path: HashMap<String, PathBuf>,
}

impl WorkerMaps {
    /// Registers a freshly loaded worker and the providers it reported.
    pub(crate) fn insert(&mut self, path: PathBuf, worker: Worker, providers: &[String]) {
        self.clear_path_routes(&path);
        for name in providers {
            self.provider_to_path.insert(name.clone(), path.clone());
        }
        self.by_path.insert(path, worker);
    }

    /// Drops the worker for `path` and every route that pointed at it.
    pub(crate) fn remove(&mut self, path: &Path) -> Option<Worker> {
        let worker = self.by_path.remove(path)?;
        self.clear_path_routes(path);
        Some(worker)
    }

    /// Takes every worker and clears all routes (app shutdown).
    pub(crate) fn take_all(&mut self) -> Vec<Worker> {
        let workers: Vec<Worker> = self.by_path.drain().map(|(_, worker)| worker).collect();
        self.provider_to_path.clear();
        self.search_to_path.clear();
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

    pub(crate) fn worker_for_provider(&self, provider: &str) -> Result<Worker, GatewayError> {
        Ok(self.worker_and_path_for_provider(provider)?.0)
    }

    /// Worker that owns `provider`, plus its plugin path (for link-search routing).
    pub(crate) fn worker_and_path_for_provider(
        &self,
        provider: &str,
    ) -> Result<(Worker, PathBuf), GatewayError> {
        let path = self
            .provider_to_path
            .get(provider)
            .cloned()
            .ok_or_else(|| {
                GatewayError::Unavailable(format!("no worker loaded for provider '{provider}'"))
            })?;
        let worker = self.by_path.get(&path).cloned().ok_or_else(|| {
            GatewayError::Unavailable(format!("worker for provider '{provider}' is gone"))
        })?;
        Ok((worker, path))
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
        maps.insert(path.clone(), Arc::new(EmptyWorker), &["Alpha".to_owned()]);
        assert!(maps.worker_for_provider("Alpha").is_ok());
        assert!(maps.remove(&path).is_some());
        assert!(maps.worker_for_provider("Alpha").is_err());
        assert!(maps.all_workers().is_empty());
    }

    #[test]
    fn remember_search_routes_poll_to_the_provider_worker() {
        let path = PathBuf::from("/ext/B.cs3");
        let mut maps = WorkerMaps::default();
        maps.insert(path.clone(), Arc::new(EmptyWorker), &["Beta".to_owned()]);
        maps.remember_search("s1".to_owned(), path);
        assert!(maps.worker_for_search("s1").is_ok());
        maps.forget_search("s1");
        assert!(maps.worker_for_search("s1").is_err());
    }
}
