//! What is kept for one cookie scope, and the rules for changing it. Pure: both pass books (memory and disk)
//! apply these rules, so they behave the same and are tested once.
//!
//! A scope holds the newest pass whose clearance has that domain, and the clearances the site has already
//! rejected (until they would have expired). A rejected clearance is never saved again, so a browser profile
//! that still holds it cannot bring it back and start a loop.

use std::time::SystemTime;

use crate::cookie_value::CookieValue;
use crate::pass_cookie::CookieDomain;
use crate::pass_ids::PassId;
use crate::saved_pass::SavedPass;

/// At most this many rejected clearances are remembered per scope (the soonest to expire go first).
pub(crate) const MAX_REJECTED_PER_SCOPE: usize = 16;

/// A clearance the site refused, remembered until it would have expired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RejectedClearance {
    pub(crate) value: CookieValue,
    pub(crate) until: SystemTime,
}

/// Everything kept for one scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeEntry {
    pub(crate) scope: CookieDomain,
    pub(crate) pass: Option<SavedPass>,
    pub(crate) rejected: Vec<RejectedClearance>,
}

/// What saving a pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Kept; it replaced the scope's older pass, if any.
    Saved {
        /// The pass it replaced.
        replaced: Option<PassId>,
    },
    /// The same clearance is already kept (under the id it was first saved with).
    AlreadyKept(PassId),
    /// The scope holds a pass obtained later; the older one is not kept.
    Outdated,
    /// The site already rejected this clearance.
    Rejected,
    /// It ends too soon to be worth keeping.
    Stale,
}

impl ScopeEntry {
    pub(crate) fn empty(scope: CookieDomain) -> Self {
        Self {
            scope,
            pass: None,
            rejected: Vec::new(),
        }
    }

    /// Keeps `pass` (which must belong to this scope) unless a rule says otherwise.
    pub(crate) fn save(&mut self, pass: SavedPass, now: SystemTime) -> SaveOutcome {
        if !pass.is_fresh(now) {
            return SaveOutcome::Stale;
        }
        if self.is_rejected(pass.clearance().value(), now) {
            return SaveOutcome::Rejected;
        }
        if let Some(kept) = &self.pass {
            if kept.clearance().same_credential(pass.clearance()) {
                return SaveOutcome::AlreadyKept(kept.id().clone());
            }
            if kept.is_fresh(now) && kept.parts().obtained_at > pass.parts().obtained_at {
                return SaveOutcome::Outdated;
            }
        }
        let replaced = self.pass.replace(pass).map(|old| old.id().clone());
        SaveOutcome::Saved { replaced }
    }

    /// The site rejected pass `id`: it is dropped and its clearance remembered. False when this scope does
    /// not hold it.
    pub(crate) fn reject(&mut self, id: &PassId) -> bool {
        let Some(pass) = self.pass.take_if(|pass| pass.id() == id) else {
            return false;
        };
        self.rejected.push(RejectedClearance {
            value: pass.clearance().value().clone(),
            until: pass.valid_until(),
        });
        self.rejected
            .sort_by_key(|rejected| std::cmp::Reverse(rejected.until));
        self.rejected.truncate(MAX_REJECTED_PER_SCOPE);
        true
    }

    /// Drops what can no longer be used; true when something was dropped.
    pub(crate) fn prune(&mut self, now: SystemTime) -> bool {
        let before = (self.pass.is_some(), self.rejected.len());
        self.pass.take_if(|pass| !pass.is_fresh(now));
        self.rejected.retain(|rejected| rejected.until > now);
        before != (self.pass.is_some(), self.rejected.len())
    }

    /// True when nothing is left to keep.
    pub(crate) fn is_empty(&self) -> bool {
        self.pass.is_none() && self.rejected.is_empty()
    }

    /// The scope's pass, if it may still be handed out.
    pub(crate) fn live_pass(&self, now: SystemTime) -> Option<&SavedPass> {
        self.pass.as_ref().filter(|pass| pass.is_fresh(now))
    }

    /// When the scope's pass was obtained (the cap evicts the oldest first); None for a scope that only
    /// remembers rejections.
    pub(crate) fn obtained_at(&self) -> Option<SystemTime> {
        self.pass.as_ref().map(|pass| pass.parts().obtained_at)
    }

    fn is_rejected(&self, value: &CookieValue, now: SystemTime) -> bool {
        self.rejected
            .iter()
            .any(|rejected| rejected.until > now && rejected.value == *value)
    }
}

/// Which of `scopes` (key, obtained) to drop so that at most `cap` remain: those that only remember
/// rejections are kept (they guard against loops), passes go oldest first.
pub(crate) fn scopes_over_cap<K: Clone>(scopes: &[(K, Option<SystemTime>)], cap: usize) -> Vec<K> {
    let mut passes: Vec<(SystemTime, &K)> = scopes
        .iter()
        .filter_map(|(key, obtained)| obtained.map(|time| (time, key)))
        .collect();
    let excess = scopes.len().saturating_sub(cap).min(passes.len());
    passes.sort_by_key(|(time, _)| *time);
    passes
        .into_iter()
        .take(excess)
        .map(|(_, key)| key.clone())
        .collect()
}

#[cfg(test)]
#[path = "pass_scope_tests.rs"]
mod tests;
