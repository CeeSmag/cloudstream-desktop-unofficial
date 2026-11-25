//! At most one install at a time, however often Install is pressed, and a way to stop it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Lets one install run at a time. Clones share the same gate.
#[derive(Debug, Clone, Default)]
pub struct InstallGate {
    active: Arc<AtomicBool>,
}

/// Proof that this caller's install is the one running; the gate opens again when it is dropped.
#[derive(Debug)]
pub struct InstallRun {
    active: Arc<AtomicBool>,
}

impl InstallGate {
    /// An open gate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the gate, or `None` when an install is already running. Checking and taking are one
    /// atomic step, so two presses cannot both start one.
    pub fn try_enter(&self) -> Option<InstallRun> {
        self.active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| InstallRun {
                active: Arc::clone(&self.active),
            })
    }

    /// Whether an install is running.
    pub fn is_busy(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

impl Drop for InstallRun {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

/// Asks a running install to stop. Clones share the same flag.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag {
    cancelled: Arc<AtomicBool>,
}

impl CancelFlag {
    /// A flag nobody has raised.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the install to stop (honoured until Microsoft's setup starts).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Whether stopping was asked for.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[cfg(test)]
#[path = "install_gate_tests.rs"]
mod tests;
