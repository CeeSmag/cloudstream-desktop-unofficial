//! How requests end: the helper's statuses, requests the provider withdrew, and a helper that exited.

use crate::{CloudflareEvent, HelperStatus, Outcome, VerifyRequest};

use super::watch::{answer, published_status, Watcher};
use super::watch_requests::{PendingRequest, RequestStage};

impl Watcher<'_> {
    pub(super) fn browser_status(&mut self, status: HelperStatus) {
        let (host, id) = match &status {
            HelperStatus::WindowOpen { .. } => return,
            HelperStatus::Passed { host, id } | HelperStatus::Failed { host, id, .. } => {
                (host.clone(), id.clone())
            }
        };
        let valid = self.requests.get(&id).is_some_and(|entry| {
            entry.matches(&host)
                && matches!(
                    entry.stage,
                    RequestStage::Helper(_) | RequestStage::Withdrawing(_)
                )
        });
        if !valid {
            return;
        }
        let Some(entry) = self.requests.remove(&id) else {
            return;
        };
        let quiet = self.owners.is_quiet(&id);
        if matches!(status, HelperStatus::Failed { .. }) {
            if matches!(entry.stage, RequestStage::Withdrawing(_)) {
                self.emit(CloudflareEvent::Dismissed { id: id.clone() });
                self.withdrew(&id);
                return;
            }
            if !quiet {
                self.requests.insert(
                    id.clone(),
                    PendingRequest::new(entry.request, RequestStage::Retry),
                );
            }
        }
        self.emit(status.into());
        self.owners.finish(&id);
    }

    /// The provider withdrew `id` while its check goes on for others: the prompt passes to the next request.
    fn withdrew(&mut self, id: &str) {
        if let Some(next) = self.owners.withdraw(id) {
            self.promotions.push(next);
        }
    }

    pub(super) fn expire_requests(&mut self) {
        let pid = self.helper.as_mut().and_then(crate::VerifyHelper::pid);
        for (id, mut entry) in std::mem::take(&mut self.requests) {
            match entry.stage {
                RequestStage::Helper(owner) if pid == Some(owner) => {
                    if !entry.live(&self.dir) {
                        self.cancel(id.clone(), owner);
                        entry.stage = RequestStage::Withdrawing(owner);
                    }
                    self.requests.insert(id, entry);
                }
                RequestStage::Withdrawing(owner) if pid == Some(owner) => {
                    self.requests.insert(id, entry);
                }
                RequestStage::Helper(_) | RequestStage::Withdrawing(_) => self.helper_exited(entry),
                _ if entry.live(&self.dir) => {
                    self.requests.insert(id, entry);
                }
                _ => self.expire(entry),
            }
        }
    }

    fn helper_exited(&mut self, entry: PendingRequest) {
        let request = &entry.request;
        match published_status(&self.dir, request) {
            Ok(Some(status)) => {
                self.requests.insert(request.id.clone(), entry);
                self.browser_status(status);
            }
            // A waiting sidecar removes the request before removing the result.
            // Check liveness after checking for the published result.
            Ok(None) if matches!(entry.stage, RequestStage::Helper(_)) && entry.live(&self.dir) => {
                self.refuse(
                    entry.request,
                    "the verification browser exited before answering",
                    true,
                );
            }
            Ok(None) => self.expire(entry),
            Err(error) => {
                tracing::warn!(id = %request.id, %error, "could not read the published verification result");
                let status = read_failure(request);
                self.requests.insert(request.id.clone(), entry);
                self.browser_status(status);
            }
        }
    }

    pub(super) fn expire(&mut self, entry: PendingRequest) {
        let id = entry.request.id;
        match entry.stage {
            RequestStage::Permission => {
                if self.dir.join(format!("request-{id}.txt")).is_file() {
                    answer(
                        &self.dir,
                        &id,
                        &Outcome::Waiting("verification was dismissed".to_owned()),
                    );
                }
            }
            RequestStage::Helper(pid) => self.cancel(id.clone(), pid),
            RequestStage::Retry | RequestStage::Withdrawing(_) => {}
        }
        self.emit(CloudflareEvent::Dismissed { id: id.clone() });
        self.withdrew(&id);
    }
}

fn read_failure(request: &VerifyRequest) -> HelperStatus {
    HelperStatus::Failed {
        host: request.host.clone(),
        id: request.id.clone(),
        reason: "the verification result could not be read".to_owned(),
    }
}
