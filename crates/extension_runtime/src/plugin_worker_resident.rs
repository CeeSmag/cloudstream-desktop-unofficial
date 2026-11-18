//! Park / revive helpers so only a few plugin JVMs stay resident.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::PoisonError;

use extension_api::GatewayError;

use crate::plugin_worker_maps::Worker;
use crate::plugin_worker_pool::PluginWorkerPool;

impl PluginWorkerPool {
    /// After a successful load of `keep`, park older live workers until under the cap.
    pub(crate) fn enforce_resident_limit(&self, keep: &Path) {
        let Some(limit) = self.limit() else {
            return;
        };
        loop {
            let to_park = {
                let maps = self.lock_maps();
                if maps.live_count() <= limit {
                    return;
                }
                maps.oldest_live_except(keep)
            };
            let Some(path) = to_park else {
                return;
            };
            match self.lock_maps().park(&path) {
                Some(worker) => {
                    tracing::info!(
                        path = %path.display(),
                        limit,
                        "parking plugin worker to stay under the resident limit"
                    );
                    worker.shutdown();
                }
                None => return,
            }
        }
    }

    /// Ensures the worker for `provider` is live, reviving a parked plugin when needed.
    pub(crate) fn live_worker_for_provider(
        &self,
        provider: &str,
    ) -> Result<(Worker, PathBuf), GatewayError> {
        // Drop the maps guard before note_used / revive — std Mutex is not re-entrant.
        let live = self.lock_maps().worker_and_path_for_provider(provider);
        if let Ok((worker, path)) = live {
            self.note_used(&path);
            return Ok((worker, path));
        }
        let path = self.lock_maps().path_for_provider(provider)?;
        if self.lock_maps().record(&path).is_none() {
            return Err(GatewayError::Unavailable(format!(
                "worker for provider '{provider}' is gone"
            )));
        }
        self.revive_path(&path)?;
        let (worker, path) = self.lock_maps().worker_and_path_for_provider(provider)?;
        self.note_used(&path);
        Ok((worker, path))
    }

    fn note_used(&self, path: &Path) {
        if self.limit().is_some() {
            self.lock_maps().touch_live(path);
        }
    }

    fn revive_path(&self, path: &Path) -> Result<(), GatewayError> {
        let _gate = self
            .revive_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Another thread may have finished reviving while we waited for the gate.
        if self.lock_maps().by_path_contains(path) {
            return Ok(());
        }
        let record = self.lock_maps().record(path).cloned().ok_or_else(|| {
            GatewayError::Unavailable(format!("no record to revive plugin at {}", path.display()))
        })?;
        tracing::info!(
            path = %path.display(),
            trusted = record.trusted,
            "reviving parked plugin worker"
        );
        let worker = (self.spawn)(record.trusted);
        if self.debug_logging.load(Ordering::SeqCst) {
            let _ = worker.set_debug_logging(true);
        }
        match worker.load_extension(path, record.trusted) {
            Ok(_providers) => {
                // Keep the original provider routes so a revive worker cannot rename them.
                self.lock_maps().insert(path.to_path_buf(), worker, record);
                self.enforce_resident_limit(path);
                Ok(())
            }
            Err(error) => {
                worker.shutdown();
                Err(error)
            }
        }
    }

    fn limit(&self) -> Option<usize> {
        let value = self.resident_limit.load(Ordering::SeqCst);
        if value == 0 {
            None
        } else {
            Some(value)
        }
    }
}
