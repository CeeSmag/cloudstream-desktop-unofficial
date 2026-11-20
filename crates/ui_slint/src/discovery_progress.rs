//! Pure progress state machine and configuration for background repository discovery.

use std::time::Duration;

/// Timing and stop limits for background repository discovery.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DiscoveryConfig {
    /// Pause between asks of the extension runtime.
    pub(crate) poll_every: Duration,
    /// Stop watching this long after the latest start or plugin-load trigger.
    pub(crate) watch_at_most: Duration,
    /// Stop after this many empty polls in a row.
    pub(crate) quiet_polls_before_stop: u32,
    /// Reload the full plugin catalog once this many quiet polls follow a discovery batch.
    pub(crate) settle_polls_before_catalog: u32,
    /// Stop after this many failed asks in a row.
    pub(crate) max_consecutive_failures: u32,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            poll_every: Duration::from_secs(2),
            watch_at_most: Duration::from_secs(300),
            quiet_polls_before_stop: 15,
            settle_polls_before_catalog: 2,
            max_consecutive_failures: 5,
        }
    }
}

/// Pure progress tracker deciding when to reload the catalog and when to stop polling.
#[derive(Debug, Default)]
pub(crate) struct WatchProgress {
    pub(crate) quiet_polls: u32,
    pub(crate) failures: u32,
    pub(crate) found_any: bool,
    pub(crate) pending_catalog_reload: bool,
}

impl WatchProgress {
    pub(crate) fn record_discovered(&mut self, added: usize) {
        self.failures = 0;
        self.quiet_polls = 0;
        self.found_any = true;
        if added > 0 {
            self.pending_catalog_reload = true;
        }
    }

    pub(crate) fn record_empty(&mut self) {
        self.failures = 0;
        self.quiet_polls = self.quiet_polls.saturating_add(1);
    }

    pub(crate) fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }

    pub(crate) fn reset_for_trigger(&mut self) {
        self.quiet_polls = 0;
        self.failures = 0;
    }

    pub(crate) fn take_catalog_reload(&mut self, settle_polls: u32) -> bool {
        if self.pending_catalog_reload && self.quiet_polls >= settle_polls {
            self.pending_catalog_reload = false;
            return true;
        }
        false
    }

    pub(crate) fn take_final_catalog_reload(&mut self) -> bool {
        std::mem::take(&mut self.pending_catalog_reload)
    }

    pub(crate) fn should_stop(&self, config: &DiscoveryConfig) -> bool {
        self.failures >= config.max_consecutive_failures
            || self.quiet_polls >= config.quiet_polls_before_stop
    }
}
