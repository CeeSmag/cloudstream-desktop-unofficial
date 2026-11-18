//! One JVM sidecar process per loaded `.cs3`, routed by provider / search id.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use core_model::{
    DiscoveredRepository, HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry,
    PlayableLinks, ProviderInfo, SearchPage,
};
use extension_api::{GatewayError, ProviderGateway};

use crate::plugin_worker_maps::{Worker, WorkerMaps};
use crate::{RpcProviderGateway, SidecarLaunchConfig, SidecarProcessTransport};

/// Spawns a fresh worker gateway (one sidecar process).
type SpawnFn = Arc<dyn Fn() -> Worker + Send + Sync>;

/// `ProviderGateway` that keeps a separate sidecar JVM for each loaded plugin file.
pub struct PluginWorkerPool {
    spawn: SpawnFn,
    maps: Mutex<WorkerMaps>,
    debug_logging: AtomicBool,
}

impl PluginWorkerPool {
    /// Production pool: each worker is a new [`SidecarProcessTransport`] with `config`.
    pub fn new(config: SidecarLaunchConfig) -> Self {
        Self::with_spawn(Arc::new(move || {
            Arc::new(RpcProviderGateway::new(SidecarProcessTransport::new(
                config.clone(),
            )))
        }))
    }

    /// Test / custom pool: `spawn` builds each worker (no real JVM required).
    pub fn with_spawn(spawn: SpawnFn) -> Self {
        Self {
            spawn,
            maps: Mutex::new(WorkerMaps::default()),
            debug_logging: AtomicBool::new(false),
        }
    }

    fn lock_maps(&self) -> std::sync::MutexGuard<'_, WorkerMaps> {
        self.maps.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn worker_for_provider(&self, provider: &str) -> Result<Worker, GatewayError> {
        self.lock_maps().worker_for_provider(provider)
    }

    fn worker_for_search(&self, id: &str) -> Result<Worker, GatewayError> {
        self.lock_maps().worker_for_search(id)
    }

    fn all_workers(&self) -> Vec<Worker> {
        self.lock_maps().all_workers()
    }

    fn drop_existing(&self, path: &Path) {
        let old = self.lock_maps().remove(path);
        if let Some(worker) = old {
            worker.shutdown();
        }
    }
}

impl ProviderGateway for PluginWorkerPool {
    fn load_extension(
        &self,
        path: &Path,
        trusted: bool,
    ) -> Result<Vec<ProviderInfo>, GatewayError> {
        self.drop_existing(path);
        let worker = (self.spawn)();
        if self.debug_logging.load(Ordering::SeqCst) {
            let _ = worker.set_debug_logging(true);
        }
        let providers = match worker.load_extension(path, trusted) {
            Ok(list) => list,
            Err(error) => {
                worker.shutdown();
                return Err(error);
            }
        };
        let names: Vec<String> = providers.iter().map(|info| info.name.clone()).collect();
        self.lock_maps().insert(path.to_path_buf(), worker, &names);
        tracing::info!(
            path = %path.display(),
            providers = names.len(),
            "plugin worker loaded"
        );
        Ok(providers)
    }

    fn unload_extension(&self, path: &Path) -> Result<(), GatewayError> {
        let worker = self.lock_maps().remove(path);
        match worker {
            Some(worker) => {
                let outcome = worker.unload_extension(path);
                worker.shutdown();
                outcome
            }
            None => Ok(()),
        }
    }

    fn search(&self, provider: &str, query: &str, page: u32) -> Result<SearchPage, GatewayError> {
        self.worker_for_provider(provider)?
            .search(provider, query, page)
    }

    fn main_page(
        &self,
        provider: &str,
        page: u32,
        entry: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        self.worker_for_provider(provider)?
            .main_page(provider, page, entry)
    }

    fn load(&self, provider: &str, url: &str) -> Result<Option<LoadDetails>, GatewayError> {
        self.worker_for_provider(provider)?.load(provider, url)
    }

    fn load_links(&self, provider: &str, data: &str) -> Result<PlayableLinks, GatewayError> {
        self.worker_for_provider(provider)?
            .load_links(provider, data)
    }

    fn start_link_search(&self, provider: &str, data: &str) -> Result<String, GatewayError> {
        let (worker, path) = self.lock_maps().worker_and_path_for_provider(provider)?;
        let id = worker.start_link_search(provider, data)?;
        self.lock_maps().remember_search(id.clone(), path);
        Ok(id)
    }

    fn poll_link_search(
        &self,
        id: &str,
        cursor: LinkSearchCursor,
        wait: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        let worker = self.worker_for_search(id)?;
        let update = worker.poll_link_search(id, cursor, wait)?;
        if update.done {
            self.lock_maps().forget_search(id);
        }
        Ok(update)
    }

    fn cancel_link_search(&self, id: &str) -> Result<(), GatewayError> {
        let worker = self.worker_for_search(id);
        self.lock_maps().forget_search(id);
        match worker {
            Ok(worker) => worker.cancel_link_search(id),
            Err(_) => Ok(()),
        }
    }

    fn extract(&self, url: &str, referer: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        let workers = self.all_workers();
        if workers.is_empty() {
            return Err(GatewayError::Unavailable(
                "no plugin workers loaded for extract".to_owned(),
            ));
        }
        fan_out_extract(&workers, url, referer)
    }

    fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, GatewayError> {
        let mut all = Vec::new();
        for worker in self.all_workers() {
            match worker.take_added_repositories() {
                Ok(found) => all.extend(found),
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "skipping a plugin worker that failed take_added_repositories"
                    );
                }
            }
        }
        Ok(all)
    }

    fn extend_call_deadline(&self, extra: Duration) {
        for worker in self.all_workers() {
            worker.extend_call_deadline(extra);
        }
    }

    fn shutdown(&self) {
        let workers = self.lock_maps().take_all();
        for worker in workers {
            worker.shutdown();
        }
    }

    fn set_debug_logging(&self, enabled: bool) -> Result<(), GatewayError> {
        self.debug_logging.store(enabled, Ordering::SeqCst);
        let mut last_error = None;
        for worker in self.all_workers() {
            if let Err(error) = worker.set_debug_logging(enabled) {
                last_error = Some(error);
            }
        }
        match last_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// First useful extract reply wins; otherwise the last error or empty success.
fn fan_out_extract(
    workers: &[Worker],
    url: &str,
    referer: Option<&str>,
) -> Result<PlayableLinks, GatewayError> {
    let mut last_ok = None;
    let mut last_err = None;
    for worker in workers {
        match worker.extract(url, referer) {
            Ok(links) if extract_useful(&links) => return Ok(links),
            Ok(links) => last_ok = Some(links),
            Err(error) => last_err = Some(error),
        }
    }
    if let Some(links) = last_ok {
        return Ok(links);
    }
    Err(last_err
        .unwrap_or_else(|| GatewayError::Unavailable("extract found no workers".to_owned())))
}

fn extract_useful(links: &PlayableLinks) -> bool {
    links.success || !links.links.is_empty()
}

#[cfg(test)]
#[path = "plugin_worker_pool_tests.rs"]
mod tests;
