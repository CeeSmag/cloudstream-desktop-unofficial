//! Request identity, routing and viewer actions for the exchange watcher.
//!
//! Requests are answered one by one (each has its own result file), but the viewer is asked once per
//! origin: the first request of an origin owns the prompt, the others are quiet (`prompt_owners`).

use std::time::{Duration, Instant};

use crate::{CloudflareEvent, HelperCommand, Outcome, VerifyRequest};

use super::watch::{answer, ViewerAction, Watcher};

const REQUEST_TIME: Duration = Duration::from_secs(4 * 60);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestStage {
    Permission,
    Helper(u32),
    Withdrawing(u32),
    Retry,
}

pub(super) struct PendingRequest {
    pub(super) request: VerifyRequest,
    pub(super) stage: RequestStage,
    pub(super) expires: Instant,
}

impl PendingRequest {
    pub(super) fn new(request: VerifyRequest, stage: RequestStage) -> Self {
        Self {
            request,
            stage,
            expires: Instant::now() + REQUEST_TIME,
        }
    }

    pub(super) fn matches(&self, host: &str) -> bool {
        self.request.host == host
    }

    pub(super) fn live(&self, exchange: &std::path::Path) -> bool {
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
        self.retire_rejected_pass(&request);
        let Some(origin) = request
            .can_navigate()
            .then(|| request.origin_key())
            .flatten()
        else {
            self.refuse(
                request,
                "this verification request cannot be opened as a browser GET",
                false,
            );
            return;
        };
        if self.answer_from_saved_pass(&request, &origin) {
            return;
        }
        if self.allowed.is_allowed(&request.host) {
            self.owners.join(&origin, &request.id);
            self.launch(request, false);
        } else if self.owners.has_owner(&origin) {
            self.defer(&request);
        } else {
            self.owners.join(&origin, &request.id);
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

    /// An earlier request of this origin already asks the viewer, so this one fails at once (the
    /// sidecar asks again next time) instead of holding a plugin thread until the viewer answers.
    fn defer(&self, request: &VerifyRequest) {
        tracing::debug!(host = %request.host, id = %request.id, "the viewer is already asked about this site; this request fails at once");
        let reason = format!(
            "{} needs a quick check; press Verify in CloudStream",
            request.host
        );
        answer(&self.dir, &request.id, &Outcome::Waiting(reason));
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

    /// The viewer dismissed the prompt: the owner and every request that waited behind it are retired.
    fn dismiss(&mut self, id: &str) {
        for follower in self.owners.dissolve(id) {
            if let Some(entry) = self.requests.remove(&follower) {
                self.expire(entry);
            }
        }
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

    pub(super) fn refuse(&mut self, request: VerifyRequest, reason: &str, retry: bool) {
        if !request.standalone {
            answer(
                &self.dir,
                &request.id,
                &Outcome::Declined(reason.to_owned()),
            );
        }
        let quiet = self.owners.is_quiet(&request.id);
        let event = CloudflareEvent::Failed {
            host: request.host.clone(),
            id: request.id.clone(),
            reason: reason.to_owned(),
            retry,
        };
        self.emit(event);
        self.owners.finish(&request.id);
        if retry && !quiet {
            self.requests.insert(
                request.id.clone(),
                PendingRequest::new(request, RequestStage::Retry),
            );
        }
    }

    /// A follower now speaks for its origin: the banner learns that the check goes on.
    pub(super) fn promote(&mut self, id: &str) {
        let Some(entry) = self.requests.get(id) else {
            return;
        };
        if matches!(entry.stage, RequestStage::Helper(_)) {
            let event = CloudflareEvent::Verifying {
                host: entry.request.host.clone(),
                id: id.to_owned(),
            };
            self.emit(event);
        }
    }
}
