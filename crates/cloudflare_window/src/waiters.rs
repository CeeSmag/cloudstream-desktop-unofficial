//! The requests that share one origin's check.
//!
//! A page asks one site for many things at once, and Cloudflare's clearance belongs to the site, so the
//! helper opens one window per origin. The request whose exact address the window opened on is the
//! owner; requests for the same origin that arrive while the check is pending wait with it. Each request
//! is answered exactly once, whether the check passes, fails, or the request is retired on its own.

use cloudflare_bridge::VerifyRequest;

/// What retiring one request left behind.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Retired {
    /// The owner went; the check goes on under the oldest waiter, who is now the owner.
    Owner(VerifyRequest),
    /// A waiter went; the check is unchanged.
    Waiter(VerifyRequest),
    /// The only request went: nobody waits for the check any more.
    Last(VerifyRequest),
    /// That request is not part of this check (it was answered already).
    Unknown,
}

/// The owner and the waiters of one check.
pub(crate) struct Waiters {
    owner: VerifyRequest,
    others: Vec<VerifyRequest>,
}

impl Waiters {
    pub(crate) fn new(owner: VerifyRequest) -> Self {
        Self {
            owner,
            others: Vec::new(),
        }
    }

    pub(crate) fn owner(&self) -> &VerifyRequest {
        &self.owner
    }

    /// True when `request` can wait for this check: same origin, and neither side is a viewer's retry
    /// (a retry asks for a fresh check of its own).
    pub(crate) fn joins(&self, request: &VerifyRequest) -> bool {
        let origin = self.owner.origin_key();
        !request.standalone
            && !self.owner.standalone
            && origin.is_some()
            && origin == request.origin_key()
    }

    pub(crate) fn join(&mut self, request: VerifyRequest) {
        if !self.holds(&request.id) {
            self.others.push(request);
        }
    }

    pub(crate) fn holds(&self, id: &str) -> bool {
        self.owner.id == id || self.others.iter().any(|other| other.id == id)
    }

    pub(crate) fn retire(&mut self, id: &str) -> Retired {
        if self.owner.id == id {
            if self.others.is_empty() {
                return Retired::Last(self.owner.clone());
            }
            let next = self.others.remove(0);
            return Retired::Owner(std::mem::replace(&mut self.owner, next));
        }
        match self.others.iter().position(|other| other.id == id) {
            Some(index) => Retired::Waiter(self.others.remove(index)),
            None => Retired::Unknown,
        }
    }

    /// Every request of the check, the owner first, for the one answer they all get.
    pub(crate) fn take_all(&mut self) -> Vec<VerifyRequest> {
        let mut all = vec![self.owner.clone()];
        all.append(&mut self.others);
        all
    }
}

#[cfg(test)]
#[path = "waiters_tests.rs"]
mod tests;
