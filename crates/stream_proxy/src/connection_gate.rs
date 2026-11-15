//! Counts the connections being served: refuses more than a limit, and lets shutdown wait until
//! the last one is done.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Admits connections up to a limit.
#[derive(Debug)]
pub(crate) struct ConnectionGate {
    open: Mutex<usize>,
    idle: Condvar,
    limit: usize,
}

/// A place at the gate; it is given back when dropped.
#[derive(Debug)]
pub(crate) struct Permit {
    gate: Arc<ConnectionGate>,
}

impl ConnectionGate {
    pub(crate) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(0),
            idle: Condvar::new(),
            limit,
        })
    }

    fn lock(&self) -> MutexGuard<'_, usize> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A place for one more connection, or `None` when the gate is full. Checking and counting
    /// are one step, so the limit holds however many threads ask at once.
    pub(crate) fn try_enter(self: &Arc<Self>) -> Option<Permit> {
        let mut open = self.lock();
        if *open >= self.limit {
            return None;
        }
        *open += 1;
        Some(Permit {
            gate: Arc::clone(self),
        })
    }

    /// How many connections are being served.
    pub(crate) fn open_count(&self) -> usize {
        *self.lock()
    }

    /// Waits until no connection is being served; false when `limit` ran out first.
    pub(crate) fn wait_idle(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut open = self.lock();
        while *open > 0 {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            open = self
                .idle
                .wait_timeout(open, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut open = self.gate.lock();
        *open = open.saturating_sub(1);
        if *open == 0 {
            self.gate.idle.notify_all();
        }
    }
}

#[cfg(test)]
#[path = "connection_gate_tests.rs"]
mod tests;
