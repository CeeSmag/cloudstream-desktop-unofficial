//! One first-come, first-served queue per pass folder inside this process.
//!
//! The pass book's `.lock` file keeps app instances apart, but an OS file lock cannot be waited on with a
//! time limit from safe Rust, so [`DiskPassBook`](crate::DiskPassBook) polls it. Polling is unfair: a thread
//! that has just released the lock takes it again at once, while a thread that is asleep between two tries
//! keeps missing the short gaps, and gives up after the time limit although the folder was never stuck.
//! Books in one process therefore queue here first, in ticket order, so only one of them at a time ever
//! tries the file lock and none can be overtaken. The time limit is left for what it is for: another app
//! instance holding the lock.
//!
//! Waiting here has no time limit. That cannot hang: the thread whose turn it is waits at most the file
//! lock's limit and then works on a few small files, so every turn ends.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError, Weak};

/// The queue of one folder.
#[derive(Debug, Default)]
pub(crate) struct FolderGate {
    turns: Mutex<Turns>,
    turn_changed: Condvar,
}

/// Tickets handed out and the ticket whose turn it is.
#[derive(Debug, Default)]
struct Turns {
    next_ticket: u64,
    serving: u64,
}

/// The folder belongs to its holder until this is dropped; then the next ticket's turn begins.
#[derive(Debug)]
pub(crate) struct Turn<'gate> {
    gate: &'gate FolderGate,
}

impl FolderGate {
    /// The queue every book on `dir` in this process shares (two spellings of one folder share one).
    pub(crate) fn for_folder(dir: &Path) -> Arc<Self> {
        let key = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_owned());
        let mut gates = recover(all_gates().lock());
        gates.retain(|_, gate| gate.strong_count() > 0);
        if let Some(gate) = gates.get(&key).and_then(Weak::upgrade) {
            return gate;
        }
        let gate = Arc::new(Self::default());
        gates.insert(key, Arc::downgrade(&gate));
        gate
    }

    /// Waits, behind everyone who came earlier, for the folder.
    pub(crate) fn enter(&self) -> Turn<'_> {
        let ticket = self.take_ticket();
        self.wait_for_turn(ticket)
    }

    fn take_ticket(&self) -> u64 {
        let mut turns = recover(self.turns.lock());
        let ticket = turns.next_ticket;
        turns.next_ticket = ticket.wrapping_add(1);
        ticket
    }

    fn wait_for_turn(&self, ticket: u64) -> Turn<'_> {
        let mut turns = recover(self.turns.lock());
        while turns.serving != ticket {
            turns = recover(self.turn_changed.wait(turns));
        }
        Turn { gate: self }
    }

    /// Tickets taken and not yet finished (the holder and everyone waiting).
    #[cfg(test)]
    fn queued(&self) -> u64 {
        let turns = recover(self.turns.lock());
        turns.next_ticket.wrapping_sub(turns.serving)
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        let mut turns = recover(self.gate.turns.lock());
        turns.serving = turns.serving.wrapping_add(1);
        drop(turns);
        self.gate.turn_changed.notify_all();
    }
}

/// Every folder's queue, held weakly so a folder no book uses any more is forgotten.
fn all_gates() -> &'static Mutex<HashMap<PathBuf, Weak<FolderGate>>> {
    static GATES: OnceLock<Mutex<HashMap<PathBuf, Weak<FolderGate>>>> = OnceLock::new();
    GATES.get_or_init(Mutex::default)
}

/// The counters stay consistent even if a thread panicked while holding the mutex: each update is one step.
fn recover<T>(locked: Result<T, PoisonError<T>>) -> T {
    locked.unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "folder_gate_tests.rs"]
mod tests;
