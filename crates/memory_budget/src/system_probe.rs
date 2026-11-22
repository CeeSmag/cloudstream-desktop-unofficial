//! What the PC offers, behind a trait so the real reading (done in `app_shell`) can be replaced
//! by fixed numbers in tests.

/// Facts about the PC that the limits depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemFacts {
    /// Total physical memory, in MiB (0 when it could not be read).
    pub total_mib: u64,
    /// Logical CPU cores.
    pub cpu_cores: usize,
}

/// Reads the PC's facts. Implemented over the OS in `app_shell`.
pub trait SystemProbe: Send + Sync {
    /// The facts, read now.
    fn facts(&self) -> SystemFacts;
}

/// A probe that always answers the same facts (tests, and PCs where nothing can be read).
#[derive(Debug, Clone, Copy)]
pub struct FixedSystem(pub SystemFacts);

impl SystemProbe for FixedSystem {
    fn facts(&self) -> SystemFacts {
        self.0
    }
}

#[cfg(test)]
#[path = "system_probe_tests.rs"]
mod tests;
