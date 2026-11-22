//! How lean each plugin's JVM runs (plan step 1b, `docs/MEMORY.md`).
//!
//! A plugin process is a JVM: about 230 MiB each, and every resident plugin pays it. The default
//! JVM is tuned for a long-running server (parallel collector, optimizing compiler, large code
//! cache, unbounded metaspace and direct memory). A plugin only scrapes a few pages, so the lean
//! profile asks for the smallest sensible footprint. `Legacy` adds nothing: the way back.

use crate::SidecarLaunchConfig;

/// The flag set a plugin JVM starts with, on top of the heap cap and sandbox options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JvmProfile {
    /// The JVM's own defaults (what shipped before this profile existed).
    Legacy,
    /// Serial collector, light compiler, bounded metaspace and direct memory.
    #[default]
    Lean,
}

/// Flags of the lean profile. `-Xmx` and the compressed class space stay in `SidecarLaunchConfig`.
const LEAN_OPTIONS: &[&str] = &[
    // One small collector thread instead of G1's concurrent threads and remembered sets; a heap
    // that starts small and gives memory back after a collection.
    "-XX:+UseSerialGC",
    "-Xms16m",
    "-XX:MinHeapFreeRatio=10",
    "-XX:MaxHeapFreeRatio=30",
    // C1 only: no C2 compiler threads, a small code cache, and no C2 memory spike on class
    // conversion. A scraper is I/O bound, so peak speed does not matter.
    "-XX:TieredStopAtLevel=1",
    "-XX:CICompilerCount=1",
    "-XX:ReservedCodeCacheSize=48m",
    // Fail fast instead of growing: metaspace and NIO buffers were unbounded.
    "-XX:MaxMetaspaceSize=128m",
    "-XX:MaxDirectMemorySize=64m",
    // No hsperfdata mapping (and no write into /tmp from inside the sandbox).
    "-XX:-UsePerfData",
    // The transport restarts a dead sidecar; a clean exit beats a JVM thrashing at its heap cap.
    "-XX:+ExitOnOutOfMemoryError",
];

impl JvmProfile {
    /// Reads `lean` or `legacy` (case and spaces ignored).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "lean" => Some(Self::Lean),
            "legacy" => Some(Self::Legacy),
            _ => None,
        }
    }

    /// JVM options this profile adds.
    pub(crate) fn java_options(self) -> &'static [&'static str] {
        match self {
            Self::Legacy => &[],
            Self::Lean => LEAN_OPTIONS,
        }
    }

    /// Environment variables this profile sets for the child process.
    pub(crate) fn environment(self) -> Vec<(&'static str, &'static str)> {
        match self {
            Self::Legacy => Vec::new(),
            // glibc gives every thread its own arena, which keeps freed memory per thread; two
            // arenas are plenty for a plugin.
            Self::Lean if cfg!(target_os = "linux") => vec![("MALLOC_ARENA_MAX", "2")],
            Self::Lean => Vec::new(),
        }
    }
}

impl SidecarLaunchConfig {
    /// Starts every plugin JVM with `profile`'s flags and environment.
    pub fn with_jvm_profile(mut self, profile: JvmProfile) -> Self {
        for option in profile.java_options() {
            self.add_java_option(option);
        }
        for (name, value) in profile.environment() {
            if !self
                .environment
                .iter()
                .any(|(existing, _)| existing == name)
            {
                self.environment.push((name.to_owned(), value.to_owned()));
            }
        }
        self
    }
}

#[cfg(test)]
#[path = "jvm_profile_tests.rs"]
mod tests;
