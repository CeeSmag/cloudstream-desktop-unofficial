//! Resident plugin-worker limit on [`ExtensionService`].

use crate::ExtensionService;

impl ExtensionService {
    /// Caps how many plugin JVMs stay alive (`None` = unlimited).
    pub fn set_resident_worker_limit(&self, limit: Option<usize>) {
        self.gateway.set_resident_worker_limit(limit);
    }
}
