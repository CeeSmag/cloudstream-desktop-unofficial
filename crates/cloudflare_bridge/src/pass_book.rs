//! Where saved passes are kept: the port the watcher uses, and an in-memory book.
//!
//! [`DiskPassBook`](crate::disk_pass_book::DiskPassBook) keeps them across restarts; [`MemoryPassBook`] keeps
//! them for one run (and is what the shared contract tests check the disk book against). Both apply the same
//! per-scope rules (`pass_scope`).

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use crate::pass_cookie::CookieDomain;
use crate::pass_ids::PassId;
use crate::pass_scope::{scopes_over_cap, SaveOutcome, ScopeEntry};
use crate::saved_pass::SavedPass;

/// At most this many scopes are kept (a viewer uses a handful of protected sites).
pub const MAX_PASSES: usize = 256;

/// Why the pass book could not be read or changed. Never holds a cookie value.
#[derive(Debug, thiserror::Error)]
pub enum PassStoreError {
    /// A file operation failed.
    #[error("could not {operation} {}: {source}", path.display())]
    Io {
        /// What was being done.
        operation: &'static str,
        /// The file or folder.
        path: PathBuf,
        /// Why it failed.
        #[source]
        source: io::Error,
    },
    /// Another app instance held the book's lock for too long.
    #[error("the pass book at {} stayed locked for {waited:?}", path.display())]
    LockTimeout {
        /// The lock file.
        path: PathBuf,
        /// How long this call waited.
        waited: Duration,
    },
}

/// How many scopes a prune emptied and how many the cap evicted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Scopes with nothing left to keep.
    pub emptied: usize,
    /// Passes dropped because the book was full.
    pub evicted: usize,
}

/// Keeps passes. Every method takes the time so the rules are exact in tests.
pub trait PassBook: Send {
    /// Every pass that may still be handed out.
    fn live_passes(&self, now: SystemTime) -> Vec<SavedPass>;

    /// Keeps `pass` (see [`SaveOutcome`] for when it is not kept).
    fn save(&self, pass: SavedPass, now: SystemTime) -> Result<SaveOutcome, PassStoreError>;

    /// The site rejected pass `id`: it is dropped and never kept again. False when the book does not hold it.
    fn reject(&self, id: &PassId, now: SystemTime) -> Result<bool, PassStoreError>;

    /// Drops what can no longer be used and evicts the oldest passes over [`MAX_PASSES`].
    fn prune(&self, now: SystemTime) -> Result<PruneReport, PassStoreError>;
}

/// Passes kept in memory for one run.
#[derive(Debug)]
pub struct MemoryPassBook {
    scopes: Mutex<HashMap<CookieDomain, ScopeEntry>>,
    cap: usize,
}

impl Default for MemoryPassBook {
    fn default() -> Self {
        Self::with_cap(MAX_PASSES)
    }
}

impl MemoryPassBook {
    /// A book that keeps at most `cap` scopes.
    pub fn with_cap(cap: usize) -> Self {
        Self {
            scopes: Mutex::new(HashMap::new()),
            cap,
        }
    }

    fn scopes(&self) -> std::sync::MutexGuard<'_, HashMap<CookieDomain, ScopeEntry>> {
        self.scopes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl PassBook for MemoryPassBook {
    fn live_passes(&self, now: SystemTime) -> Vec<SavedPass> {
        self.scopes()
            .values()
            .filter_map(|entry| entry.live_pass(now).cloned())
            .collect()
    }

    fn save(&self, pass: SavedPass, now: SystemTime) -> Result<SaveOutcome, PassStoreError> {
        let scope = pass.scope().clone();
        let mut scopes = self.scopes();
        let entry = scopes
            .entry(scope.clone())
            .or_insert_with(|| ScopeEntry::empty(scope));
        let outcome = entry.save(pass, now);
        evict_over_cap(&mut scopes, self.cap);
        Ok(outcome)
    }

    fn reject(&self, id: &PassId, _now: SystemTime) -> Result<bool, PassStoreError> {
        Ok(self.scopes().values_mut().any(|entry| entry.reject(id)))
    }

    fn prune(&self, now: SystemTime) -> Result<PruneReport, PassStoreError> {
        let mut scopes = self.scopes();
        for entry in scopes.values_mut() {
            entry.prune(now);
        }
        let before = scopes.len();
        scopes.retain(|_, entry| !entry.is_empty());
        let emptied = before - scopes.len();
        let evicted = evict_over_cap(&mut scopes, self.cap);
        Ok(PruneReport { emptied, evicted })
    }
}

fn evict_over_cap(scopes: &mut HashMap<CookieDomain, ScopeEntry>, cap: usize) -> usize {
    let listed: Vec<(CookieDomain, Option<SystemTime>)> = scopes
        .iter()
        .map(|(key, entry)| (key.clone(), entry.obtained_at()))
        .collect();
    let evicted = scopes_over_cap(&listed, cap);
    for key in &evicted {
        scopes.remove(key);
    }
    evicted.len()
}

#[cfg(test)]
#[path = "pass_book_contract_tests.rs"]
pub(crate) mod contract_tests;
