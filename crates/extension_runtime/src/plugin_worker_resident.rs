//! Park / revive helpers so only a few plugin JVMs stay resident.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use extension_api::GatewayError;

use crate::plugin_worker_maps::Worker;
use crate::plugin_worker_pool::PluginWorkerPool;

/// How many times a call looks for its plugin's worker when others keep parking it.
const LEASE_ATTEMPTS: usize = 8;

/// A call in progress on a plugin's worker. While it lives the worker is not parked, so another
/// plugin's revive can never cut the call short; when it ends, a worker kept above the limit for
/// its sake is parked.
pub(crate) struct WorkerLease<'a> {
    pool: &'a PluginWorkerPool,
    path: PathBuf,
    worker: Worker,
}

impl WorkerLease<'_> {
    pub(crate) fn worker(&self) -> &Worker {
        &self.worker
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkerLease<'_> {
    fn drop(&mut self) {
        self.pool.lock_maps().release(&self.path);
        self.pool.enforce_resident_limit(Path::new(""));
    }
}

impl PluginWorkerPool {
    /// After a successful load of `keep`, park older idle workers until under the cap.
    pub(crate) fn enforce_resident_limit(&self, keep: &Path) {
        let Some(limit) = self.limit() else {
            return;
        };
        self.park_oldest_idle_while(keep, |live| live > limit);
    }

    /// Makes room for one more worker under the limit: the oldest idle one stops before a new
    /// one starts, so the live count does not pass the limit while nothing is in flight.
    pub(crate) fn make_room(&self) {
        let Some(limit) = self.limit() else {
            return;
        };
        self.park_oldest_idle_while(Path::new(""), |live| live >= limit);
    }

    /// Parks the oldest idle worker (not `keep`, no call, no open search) while `too_many(live)`.
    /// Choosing and parking happen under one lock; only the shutdown happens outside it.
    fn park_oldest_idle_while(&self, keep: &Path, too_many: impl Fn(usize) -> bool) {
        loop {
            let parked = {
                let mut maps = self.lock_maps();
                if !too_many(maps.live_count()) {
                    return;
                }
                maps.park_oldest_idle(keep)
            };
            let Some((path, worker)) = parked else {
                return; // everything left is busy; the last call to end parks the excess
            };
            tracing::info!(path = %path.display(), "parking plugin worker to stay under the resident limit");
            worker.shutdown();
        }
    }

    /// The live worker for `provider`, reviving a parked plugin when needed, protected from
    /// parking until the returned lease is dropped.
    pub(crate) fn live_worker_for_provider(
        &self,
        provider: &str,
    ) -> Result<WorkerLease<'_>, GatewayError> {
        for _ in 0..LEASE_ATTEMPTS {
            // The maps guard is a temporary of each statement: std Mutex is not re-entrant.
            let leased = self.lock_maps().lease_provider(provider)?;
            if let Some((worker, path)) = leased {
                return Ok(self.lease(path, worker));
            }
            let path = self.lock_maps().path_for_provider(provider)?;
            if self.lock_maps().record(&path).is_none() {
                return Err(GatewayError::Unavailable(format!(
                    "worker for provider '{provider}' is gone"
                )));
            }
            if let Some(worker) = self.revive_path(&path)? {
                return Ok(self.lease(path, worker));
            }
        }
        Err(GatewayError::Unavailable(format!(
            "the plugin of provider '{provider}' keeps being parked; try again"
        )))
    }

    fn lease(&self, path: PathBuf, worker: Worker) -> WorkerLease<'_> {
        WorkerLease {
            pool: self,
            path,
            worker,
        }
    }

    /// Starts the parked plugin at `path` again. Returns its worker, already counted as in use by
    /// this caller, or `None` when another thread had revived it first.
    fn revive_path(&self, path: &Path) -> Result<Option<Worker>, GatewayError> {
        let _gate = self
            .revive_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Another thread may have finished reviving while we waited for the gate.
        if self.lock_maps().by_path_contains(path) {
            return Ok(None);
        }
        let record = self.lock_maps().record(path).cloned().ok_or_else(|| {
            GatewayError::Unavailable(format!("no record to revive plugin at {}", path.display()))
        })?;
        tracing::info!(path = %path.display(), trusted = record.trusted, "reviving parked plugin worker");
        self.make_room();
        let worker = (self.spawn)(record.trusted);
        if self.debug_logging.load(Ordering::SeqCst) {
            let _ = worker.set_debug_logging(true);
        }
        match worker.load_extension(path, record.trusted) {
            Ok(_providers) => {
                // Keep the original provider routes so a revive worker cannot rename them.
                self.lock_maps()
                    .insert_leased(path.to_path_buf(), Arc::clone(&worker), record);
                self.enforce_resident_limit(path);
                Ok(Some(worker))
            }
            Err(error) => {
                worker.shutdown();
                Err(error)
            }
        }
    }

    pub(crate) fn limit(&self) -> Option<usize> {
        let value = self.resident_limit.load(Ordering::SeqCst);
        if value == 0 {
            None
        } else {
            Some(value)
        }
    }

    /// Stores `limit` and parks excess idle workers (oldest first).
    pub(crate) fn apply_resident_worker_limit(&self, limit: Option<usize>) {
        let value = limit.unwrap_or(0);
        self.resident_limit.store(value, Ordering::SeqCst);
        tracing::info!(?limit, "plugin resident worker limit set");
        if let Some(cap) = limit {
            self.park_oldest_idle_while(Path::new(""), |live| live > cap);
        }
    }
}

#[cfg(test)]
#[path = "plugin_worker_lease_tests.rs"]
mod lease_tests;
