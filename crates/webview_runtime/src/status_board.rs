//! The newest known [`RuntimeStatus`], shared between the UI and its workers.
//!
//! A probe takes a ticket before it reads the registry and hands it back with its answer. Only
//! the newest ticket may write: a slow probe started at launch cannot overwrite what the probe
//! after an install found. Taking a ticket and writing both happen under one lock.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::RuntimeStatus;

/// One probe's right to write its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeTicket(u64);

#[derive(Debug, Default)]
struct Board {
    generation: u64,
    status: Option<RuntimeStatus>,
}

/// The newest status, written only by the newest probe.
#[derive(Debug, Default)]
pub struct StatusBoard {
    board: Mutex<Board>,
}

impl StatusBoard {
    /// An empty board (no probe has answered yet).
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Board> {
        // The board holds plain values; a panic elsewhere cannot leave it half-written.
        self.board.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a probe; every earlier ticket stops counting.
    pub fn begin_probe(&self) -> ProbeTicket {
        let mut board = self.lock();
        board.generation += 1;
        ProbeTicket(board.generation)
    }

    /// Writes a probe's answer if no newer probe began meanwhile. Returns whether it was written.
    pub fn finish_probe(&self, ticket: ProbeTicket, status: RuntimeStatus) -> bool {
        let mut board = self.lock();
        let current = board.generation == ticket.0;
        if current {
            board.status = Some(status);
        }
        current
    }

    /// The newest answer, `None` until a probe has answered.
    pub fn current(&self) -> Option<RuntimeStatus> {
        self.lock().status.clone()
    }
}

#[cfg(test)]
#[path = "status_board_tests.rs"]
mod tests;
