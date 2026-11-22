//! Reads this PC's RAM and cores for the Auto performance profile.

use memory_budget::{SystemFacts, SystemProbe};
use sysinfo::System;

/// Bytes in a MiB.
const BYTES_PER_MIB: u64 = 1024 * 1024;

/// The real PC, read through `sysinfo` (works the same on Windows and Linux, no `unsafe` here).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SysinfoProbe;

impl SystemProbe for SysinfoProbe {
    fn facts(&self) -> SystemFacts {
        SystemFacts {
            total_mib: read_total_mib(),
            cpu_cores: std::thread::available_parallelism().map_or(1, usize::from),
        }
    }
}

/// Total RAM in MiB; 0 (unknown, treated as a low-end PC) when the OS would not say.
fn read_total_mib() -> u64 {
    match System::new() {
        Ok(mut system) => {
            system.refresh_memory();
            system.total_memory() / BYTES_PER_MIB
        }
        Err(error) => {
            tracing::warn!(%error, "could not read this PC's memory; assuming a low-end PC");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_real_pc_reports_some_memory_and_at_least_one_core() {
        let facts = SysinfoProbe.facts();
        assert!(facts.total_mib > 0, "no RAM read: {facts:?}");
        assert!(facts.cpu_cores >= 1);
    }
}
