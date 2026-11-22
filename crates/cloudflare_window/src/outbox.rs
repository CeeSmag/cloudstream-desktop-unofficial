//! Everything the helper hands back: answers for the sidecar (files) and statuses for the app (stdout).

use std::io::Write;
use std::path::PathBuf;

use cloudflare_bridge::{
    format_status, write_fetch_result, write_result, FetchOutcome, HelperStatus, Outcome,
    VerifyRequest,
};

/// Where answers go.
pub(crate) struct Outbox {
    pub(crate) exchange: PathBuf,
}

impl Outbox {
    pub(crate) fn answer(&self, request: &VerifyRequest, outcome: &Outcome) -> Result<(), String> {
        if request.standalone {
            return Ok(());
        }
        write_result(&self.exchange, &request.id, outcome).map(|_| ()).map_err(|error| {
            tracing::error!(%error, id = %request.id, "could not deliver the verification result");
            "the verification result could not be delivered".to_owned()
        })
    }

    /// Answers one request and says how it went, for the status the app is told.
    pub(crate) fn deliver(&self, request: &VerifyRequest, outcome: &Outcome) -> HelperStatus {
        let (host, id) = (request.host.clone(), request.id.clone());
        match (outcome, self.answer(request, outcome)) {
            (_, Err(reason)) => HelperStatus::Failed { host, id, reason },
            (Outcome::Declined(reason) | Outcome::Waiting(reason), Ok(())) => {
                HelperStatus::Failed {
                    host,
                    id,
                    reason: reason.clone(),
                }
            }
            (Outcome::Passed { .. } | Outcome::BrowserClear { .. }, Ok(())) => {
                HelperStatus::Passed { host, id }
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
