//! Request identity, expiry, and viewer actions for the exchange watcher.

use std::time::{Duration, Instant};

use crate::{CloudflareEvent, HelperCommand, HelperStatus, Outcome, VerifyHelper, VerifyRequest};

use super::watch::{answer, published_status, ViewerAction, Watcher};

const REQUEST_TIME: Duration = Duration::from_secs(4 * 60);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestStage {
    Permission,
    Helper(u32),
    Withdrawing(u32),
    Retry,
}

pub(super) struct PendingRequest {
    request: VerifyRequest,
    stage: RequestStage,
    expires: Instant,
}

impl PendingRequest {
    fn new(request: VerifyRequest, stage: RequestStage) -> Self {
        Self {
            request,
            stage,
            expires: Instant::now() + REQUEST_TIME,
        }
    }

    fn matches(&self, host: &str) -> bool {
        self.request.host == host
    }

    fn live(&self, exchange: &std::path::Path) -> bool {
        Instant::now() < self.expires
            && (self.request.standalone
                || self.stage == RequestStage::Retry
                || exchange
                    .join(format!("request-{}.txt", self.request.id))
                    .is_file())
    }
}

impl Watcher<'_> {
    pub(super) fn route(&mut self, request: VerifyRequest) {
        tracing::debug!(host = %request.host, id = %request.id, "verification request arrived");
        if !request.can_navigate() {
            self.refuse(
                request,
                "this verification request cannot be opened as a browser GET",
                false,
            );
        } else if self.allowed.is_allowed(&request.host) {
            self.launch(request, false);
        } else {
            let event = CloudflareEvent::NeedsPermission {
                host: request.host.clone(),
                id: request.id.clone(),
            };
            self.requests.insert(
                request.id.clone(),
                PendingRequest::new(request, RequestStage::Permission),
            );
            self.emit(event);
        }
    }

    pub(super) fn viewer_action(&mut self, action: ViewerAction) {
        match action {
            ViewerAction::Verify(id) => self.verify(&id),
            ViewerAction::Retry { failed, attempt } => self.retry(&failed, attempt),
            ViewerAction::Show(id) => self.show(&id),
            ViewerAction::Dismiss(id) => self.dismiss(&id),
        }
    }

    fn verify(&mut self, id: &str) {
        let Some(entry) = self.requests.remove(id) else {
            self.emit(CloudflareEvent::Dismissed { id: id.to_owned() });
            return;
        };
        if !entry.live(&self.dir) {
            self.expire(entry);
            return;
        }
        match entry.stage {
            RequestStage::Permission => {
                self.allowed.allow(&entry.request.host);
                self.emit(CloudflareEvent::PermissionGranted {
                    host: entry.request.host.clone(),
                });
                self.launch(entry.request, true);
            }
            RequestStage::Helper(_) => {
                self.requests.insert(id.to_owned(), entry);
                self.show(id);
            }
            RequestStage::Withdrawing(_) | RequestStage::Retry => {
                self.requests.insert(id.to_owned(), entry);
            }
        }
    }

    fn retry(&mut self, failed: &str, attempt: String) {
        if let std::collections::hash_map::Entry::Occupied(entry) =
            self.requests.entry(failed.to_owned())
        {
            if entry.get().stage == RequestStage::Retry && entry.get().live(&self.dir) {
                let mut request = entry.remove().request;
                request.id = attempt;
                request.standalone = true;
                self.launch(request, true);
                return;
            }
        }
        self.emit(CloudflareEvent::Dismissed { id: attempt });
    }

    fn show(&mut self, id: &str) {
        let Some(entry) = self.requests.get(id) else {
            self.emit(CloudflareEvent::Dismissed { id: id.to_owned() });
            return;
        };
        if !entry.live(&self.dir) {
            self.dismiss(id);
            return;
        }
        if let (RequestStage::Helper(pid), Some(helper)) = (&entry.stage, self.helper.as_mut()) {
            if helper.pid() == Some(*pid) {
                if let Err(error) = helper.show_if_running(&entry.request.host, id) {
                    tracing::warn!(%id, %error, "could not present the verification window");
                }
            }
        }
    }

    fn dismiss(&mut self, id: &str) {
        if let Some(entry) = self.requests.remove(id) {
            self.expire(entry);
        }
    }

    fn launch(&mut self, request: VerifyRequest, present: bool) {
        let command = if present {
            HelperCommand::Present(request.clone())
        } else {
            HelperCommand::Verify(request.clone())
        };
        let result = self
            .helper
            .as_mut()
            .ok_or_else(|| std::io::Error::other("this build has no verification window"))
            .and_then(|helper| helper.send(&command));
        match result {
            Ok(pid) => {
                let event = CloudflareEvent::Verifying {
                    host: request.host.clone(),
                    id: request.id.clone(),
                };
                self.requests.insert(
                    request.id.clone(),
                    PendingRequest::new(request, RequestStage::Helper(pid)),
                );
                self.emit(event);
            }
            Err(error) => {
                tracing::warn!(id = %request.id, %error, "cannot start verification");
                self.refuse(request, "the verification window could not open", true);
            }
        }
    }

    fn refuse(&mut self, request: VerifyRequest, reason: &str, retry: bool) {
        if !request.standalone {
            answer(
                &self.dir,
                &request.id,
                &Outcome::Declined(reason.to_owned()),
            );
        }
        let event = CloudflareEvent::Failed {
            host: request.host.clone(),
            id: request.id.clone(),
            reason: reason.to_owned(),
            retry,
        };
        if retry {
            self.requests.insert(
                request.id.clone(),
                PendingRequest::new(request, RequestStage::Retry),
            );
        }
        self.emit(event);
    }

    pub(super) fn browser_status(&mut self, status: HelperStatus) {
        let (host, id) = match &status {
            HelperStatus::WindowOpen { .. } => return,
            HelperStatus::Passed { host, id } | HelperStatus::Failed { host, id, .. } => (host, id),
        };
        let valid = self.requests.get(id).is_some_and(|entry| {
            entry.matches(host)
                && matches!(
                    entry.stage,
                    RequestStage::Helper(_) | RequestStage::Withdrawing(_)
                )
        });
        if !valid {
            return;
        }
        if let Some(entry) = self.requests.remove(id) {
            if matches!(status, HelperStatus::Failed { .. }) {
                if matches!(entry.stage, RequestStage::Withdrawing(_)) {
                    self.emit(CloudflareEvent::Dismissed { id: id.clone() });
                    return;
                }
                self.requests.insert(
                    id.clone(),
                    PendingRequest::new(entry.request, RequestStage::Retry),
                );
            }
            self.emit(status.into());
        }
    }

    pub(super) fn expire_requests(&mut self) {
        let pid = self.helper.as_mut().and_then(VerifyHelper::pid);
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
                let status = HelperStatus::Failed {
                    host: request.host.clone(),
                    id: request.id.clone(),
                    reason: "the verification result could not be read".to_owned(),
                };
                self.requests.insert(request.id.clone(), entry);
                self.browser_status(status);
            }
        }
    }

    fn expire(&mut self, entry: PendingRequest) {
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
        self.emit(CloudflareEvent::Dismissed { id });
    }
}
