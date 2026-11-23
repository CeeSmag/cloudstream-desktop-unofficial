//! The watcher's use of saved passes: answer a request at once from one, keep the pass a check produced,
//! retire a pass the site refused.
//!
//! Everything here runs on the watcher thread, the only writer of the pass book in this app instance.
//! Nothing here logs a cookie value: passes are logged by id, scope and lifetime.

use std::path::PathBuf;
use std::time::SystemTime;

use url::Url;

use crate::disk_pass_book::DiskPassBook;
use crate::exchange::write_saved_result;
use crate::issued_pass::IssuedPass;
use crate::pass_book::{PassBook, PassStoreError};
use crate::pass_ids::{PassIds, RunId};
use crate::pass_policy::{decide, PassDecision, PassQuery};
use crate::reask_guard::ReaskGuard;
use crate::saved_pass::{PassParts, SavedPass};
use crate::wall_clock::{SystemClock, WallClock};
use crate::{RejectedPass, VerifyRequest};

use super::watch::Watcher;

/// Saved passes for the watcher: where they are kept, the clock their lifetimes are read against, and
/// this run of the app (its session cookies end with it).
pub struct SavedPasses {
    /// Where passes are kept.
    pub book: Box<dyn PassBook>,
    /// The wall clock.
    pub clock: Box<dyn WallClock>,
    /// This run of the app.
    pub run: RunId,
}

impl SavedPasses {
    /// Passes kept in `dir` across restarts, read against the system clock.
    pub fn on_disk(dir: impl Into<PathBuf>) -> Result<Self, PassStoreError> {
        Ok(Self {
            book: Box::new(DiskPassBook::open(dir)?),
            clock: Box::new(SystemClock),
            run: RunId::for_run(SystemTime::now(), std::process::id()),
        })
    }
}

/// The watcher's saved-pass state.
pub(super) struct PassDesk {
    passes: SavedPasses,
    ids: PassIds,
    guard: ReaskGuard,
}

impl PassDesk {
    pub(super) fn new(passes: SavedPasses) -> Self {
        let ids = PassIds::new(passes.run.clone());
        let desk = Self {
            passes,
            ids,
            guard: ReaskGuard::default(),
        };
        desk.prune();
        desk
    }

    fn now(&self) -> SystemTime {
        self.passes.clock.now()
    }

    fn prune(&self) {
        match self.passes.book.prune(self.now()) {
            Ok(report) if report != crate::PruneReport::default() => {
                tracing::info!(
                    emptied = report.emptied,
                    evicted = report.evicted,
                    "pruned saved Cloudflare passes"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "could not prune the saved Cloudflare passes"),
        }
    }
}

impl Watcher<'_> {
    /// The sidecar reports that a saved pass was refused: it is never handed out again.
    pub(super) fn retire_rejected_pass(&mut self, request: &VerifyRequest) {
        let (RejectedPass::Named(id), Some(desk)) =
            (&request.reason.rejected, self.passes.as_ref())
        else {
            return;
        };
        match desk.passes.book.reject(id, desk.now()) {
            Ok(true) => {
                tracing::info!(pass = %id, host = %request.host, request = %request.id, "the site refused a saved Cloudflare pass; it is retired and the site is checked again")
            }
            Ok(false) => {
                tracing::debug!(pass = %id, host = %request.host, "a refused pass was already retired")
            }
            Err(error) => {
                tracing::warn!(pass = %id, %error, "could not retire a refused Cloudflare pass")
            }
        }
    }

    /// Answers `request` at once from a saved pass; false when it must be routed as before.
    pub(super) fn answer_from_saved_pass(&mut self, request: &VerifyRequest, origin: &str) -> bool {
        let Some(desk) = self.passes.as_mut() else {
            return false;
        };
        let now = desk.now();
        let live = desk.passes.book.live_passes(now);
        let decision = decide(&PassQuery {
            request,
            origin,
            passes: &live,
            allowed: &self.allowed,
            guard: &desk.guard,
            now,
            run: &desk.passes.run,
        });
        let served = match decision {
            PassDecision::Serve(served) => served,
            PassDecision::Bypass(reason) => {
                tracing::debug!(host = %request.host, request = %request.id, reason = reason.as_str(), "not answered from a saved pass");
                return false;
            }
        };
        if let Err(error) = write_saved_result(&self.dir, &request.id, &served) {
            tracing::warn!(%error, request = %request.id, "could not hand a saved pass over; checking the site instead");
            return false;
        }
        desk.guard.note_served(origin, &served.id, now);
        let left = served.valid_until.duration_since(now).unwrap_or_default();
        tracing::info!(host = %request.host, request = %request.id, pass = %served.id, cookies = served.cookies.len(), valid_for_secs = left.as_secs(), "answered from a saved Cloudflare pass");
        true
    }

    /// Keeps the pass a check produced for `request`.
    pub(super) fn remember_pass(&mut self, request: &VerifyRequest, issued: IssuedPass) {
        let Some(desk) = self.passes.as_ref() else {
            return;
        };
        let Ok(issued_for) = Url::parse(&request.url) else {
            return;
        };
        let now = desk.now();
        let parts = PassParts {
            id: desk.ids.next_id(),
            issued_for,
            user_agent: issued.user_agent,
            cookies: issued.cookies,
            obtained_at: now,
            run: desk.passes.run.clone(),
        };
        let pass = match SavedPass::new(parts) {
            Ok(pass) => pass,
            Err(refusal) => {
                tracing::debug!(host = %request.host, request = %request.id, %refusal, "the check's pass is not kept");
                return;
            }
        };
        let (id, scope) = (pass.id().clone(), pass.scope().to_browser());
        match desk.passes.book.save(pass, now) {
            Ok(outcome) => {
                tracing::info!(host = %request.host, pass = %id, %scope, ?outcome, "kept the check's Cloudflare pass")
            }
            Err(error) => {
                tracing::warn!(host = %request.host, %error, "could not keep the check's Cloudflare pass")
            }
        }
    }
}

#[cfg(test)]
#[path = "watch_passes_tests.rs"]
mod tests;
