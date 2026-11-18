//! Everything the helper hands back: answers for the sidecar (files) and statuses for the app (stdout).

use std::io::Write;
use std::path::PathBuf;

use cloudflare_bridge::{
    format_status, write_fetch_result, write_result, FetchOutcome, HelperStatus, Outcome,
};

/// Where answers go.
pub(crate) struct Outbox {
    pub(crate) exchange: PathBuf,
}

impl Outbox {
    /// Hands `outcome` to every request in `ids`.
    pub(crate) fn answer_all(&self, ids: &[String], outcome: &Outcome) {
        for id in ids {
            if let Err(error) = write_result(&self.exchange, id, outcome) {
                tracing::warn!(%error, %id, "could not answer a verification request");
            }
        }
    }

    /// Hands a browser fetch's outcome to the sidecar.
    pub(crate) fn fetch_result(&self, id: &str, outcome: &FetchOutcome) {
        if let Err(error) = write_fetch_result(&self.exchange, id, outcome) {
            tracing::warn!(%error, %id, "could not answer a browser fetch");
        }
    }

    /// Tells the app how a site's check goes.
    pub(crate) fn status(&self, status: &HelperStatus) {
        let mut stdout = std::io::stdout().lock();
        let written = stdout
            .write_all(format_status(status).as_bytes())
            .and_then(|()| stdout.flush());
        if let Err(error) = written {
            tracing::warn!(%error, "the app no longer reads the helper's statuses");
        }
    }
}
