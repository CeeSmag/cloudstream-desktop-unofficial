//! Pure state machine deciding which Cloudflare check the banner displays.
//!
//! The banner shows one site at a time while additional sites wait behind it (`+N more`).
//! Priority order is: `NeedsPermission` > `Failed` > `Verifying` > `Verified`.
//!
//! Key invariants:
//! - A terminal `Verified` confirmation carries a mandatory expiry deadline and **never** blocks
//!   any active waiting prompt (`NeedsPermission`, `Failed`, or `Verifying`).
//! - A running `Verifying` check yields to an incoming `NeedsPermission` prompt and returns to the
//!   queue preserving its original arrival order.
//! - A newer active check or successful verification for a host clears any stale `Failed` prompt
//!   for that same host.

use std::time::{Duration, Instant};

use cloudflare_bridge::CloudflareEvent;

/// How long a terminal `"<host> is verified."` confirmation stays visible when no other check waits.
pub(crate) const VERIFIED_SHOWN: Duration = Duration::from_secs(6);

/// The phase of one site's Cloudflare verification prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromptPhase {
    /// Waiting for the viewer to approve opening a verification window for this site.
    NeedsPermission,
    /// A verification browser check is in flight.
    Verifying,
    /// The check failed; `can_retry` controls whether the viewer can launch a fresh attempt.
    Failed { reason: String, can_retry: bool },
    /// Brief terminal confirmation shown only when no active check is waiting, until `until`.
    Verified { until: Instant },
}

impl PromptPhase {
    /// Display priority: lower number wins the banner.
    fn priority(&self) -> u8 {
        match self {
            Self::NeedsPermission => 0,
            Self::Failed { .. } => 1,
            Self::Verifying => 2,
            Self::Verified { .. } => 3,
        }
    }

    /// True when the prompt is actionable or in flight (not a terminal confirmation).
    fn is_active(&self) -> bool {
        !matches!(self, Self::Verified { .. })
    }

    /// True when an `incoming` prompt should immediately take the banner from `self`.
    fn yields_to(&self, incoming: &Self) -> bool {
        match self {
            Self::Verified { .. } => incoming.is_active(),
            Self::Verifying => matches!(incoming, Self::NeedsPermission),
            Self::NeedsPermission | Self::Failed { .. } => false,
        }
    }
}

/// One site's verification prompt tracked by [`PromptQueue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudflarePrompt {
    pub(crate) request_id: String,
    pub(crate) host: String,
    pub(crate) phase: PromptPhase,
}

/// Coarse display state projected to the Slint banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BannerPhase {
    Hidden,
    NeedsPermission,
    Verifying,
    Verified,
    Failed,
}

/// Immutable projection of [`PromptQueue`] rendered into `CloudflareBannerState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BannerSnapshot {
    pub(crate) phase: BannerPhase,
    pub(crate) host: String,
    pub(crate) message: String,
    pub(crate) can_retry: bool,
    pub(crate) waiting_count: usize,
    pub(crate) can_cycle: bool,
}

/// Action sent to `cloudflare_bridge::WatchHandle` in response to a viewer click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BannerCommand {
    Verify { id: String },
    Show { id: String },
    Retry { id: String },
    Dismiss { id: String },
}

/// Side-effect summary returned by [`PromptQueue::apply_event`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ApplyOutcome {
    /// Host just approved by the viewer, to be persisted in settings.
    pub(crate) granted_host: Option<String>,
    /// True when a verification check passed.
    pub(crate) passed: bool,
    /// True when the visible banner snapshot changed and needs re-rendering.
    pub(crate) changed: bool,
}

impl ApplyOutcome {
    fn granted(host: String) -> Self {
        Self {
            granted_host: Some(host),
            passed: false,
            changed: false,
        }
    }
}

struct QueuedPrompt {
    prompt: CloudflarePrompt,
    order: u64,
}

/// The prompt currently displayed on the banner and the prompts waiting behind it.
#[derive(Default)]
pub(crate) struct PromptQueue {
    shown: Option<QueuedPrompt>,
    waiting: Vec<QueuedPrompt>,
    next_order: u64,
}

impl PromptQueue {
    /// Current immutable view-model for rendering the banner.
    pub(crate) fn snapshot(&self) -> BannerSnapshot {
        let Some(shown) = &self.shown else {
            return BannerSnapshot {
                phase: BannerPhase::Hidden,
                host: String::new(),
                message: String::new(),
                can_retry: false,
                waiting_count: 0,
                can_cycle: false,
            };
        };
        let (phase, message, can_retry) = match &shown.prompt.phase {
            PromptPhase::NeedsPermission => (BannerPhase::NeedsPermission, String::new(), false),
            PromptPhase::Verifying => (BannerPhase::Verifying, String::new(), false),
            PromptPhase::Verified { .. } => (BannerPhase::Verified, String::new(), false),
            PromptPhase::Failed { reason, can_retry } => {
                (BannerPhase::Failed, reason.clone(), *can_retry)
            }
        };
        let priority = shown.prompt.phase.priority();
        let can_cycle = shown.prompt.phase.is_active()
            && self
                .waiting
                .iter()
                .any(|entry| entry.prompt.phase.priority() == priority);
        BannerSnapshot {
            phase,
            host: shown.prompt.host.clone(),
            message,
            can_retry,
            waiting_count: self.waiting.len(),
            can_cycle,
        }
    }

    /// Applies one watcher event at `now` and reconciles what should be on screen.
    pub(crate) fn apply_event(&mut self, event: CloudflareEvent, now: Instant) -> ApplyOutcome {
        let before = self.snapshot();
        let mut passed = false;
        match event {
            CloudflareEvent::PermissionGranted { host } => return ApplyOutcome::granted(host),
            CloudflareEvent::NeedsPermission { host, id } => {
                self.raise_fresh(id, host, PromptPhase::NeedsPermission);
            }
            CloudflareEvent::Verifying { host, id } => {
                self.raise_fresh(id, host, PromptPhase::Verifying);
            }
            CloudflareEvent::Failed {
                host,
                id,
                reason,
                retry,
            } => {
                let phase = PromptPhase::Failed {
                    reason,
                    can_retry: retry,
                };
                self.raise(CloudflarePrompt {
                    request_id: id,
                    host,
                    phase,
                });
            }
            CloudflareEvent::Passed { host, id } => {
                passed = true;
                self.clear_failed_for_host(&host, &id);
                self.settle_passed(host, id, now);
            }
            CloudflareEvent::Dismissed { id } => self.remove_id(&id),
        }
        self.reconcile();
        ApplyOutcome {
            granted_host: None,
            passed,
            changed: before != self.snapshot(),
        }
    }

    /// Expires a shown `Verified` confirmation whose deadline has elapsed at `now`.
    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        let expired = self.shown.as_ref().is_some_and(
            |shown| matches!(shown.prompt.phase, PromptPhase::Verified { until } if now >= until),
        );
        if !expired {
            return false;
        }
        self.shown = None;
        self.reconcile();
        true
    }

    /// Command for the banner's primary button (`Verify`, `Open verification`, or `Try again`).
    pub(crate) fn primary_action(&self) -> Option<BannerCommand> {
        let shown = &self.shown.as_ref()?.prompt;
        match shown.phase {
            PromptPhase::NeedsPermission => Some(BannerCommand::Verify {
                id: shown.request_id.clone(),
            }),
            PromptPhase::Verifying => Some(BannerCommand::Show {
                id: shown.request_id.clone(),
            }),
            PromptPhase::Failed {
                can_retry: true, ..
            } => Some(BannerCommand::Retry {
                id: shown.request_id.clone(),
            }),
            PromptPhase::Failed {
                can_retry: false, ..
            }
            | PromptPhase::Verified { .. } => None,
        }
    }

    /// Transitions the shown retryable `Failed` prompt into `Verifying` under `new_id`.
    pub(crate) fn begin_retry(&mut self, failed_id: &str, new_id: String) -> bool {
        let Some(shown) = self.shown.as_mut() else {
            return false;
        };
        let retryable = shown.prompt.request_id == failed_id
            && matches!(
                shown.prompt.phase,
                PromptPhase::Failed {
                    can_retry: true,
                    ..
                }
            );
        if !retryable {
            return false;
        }
        shown.prompt.request_id = new_id;
        shown.prompt.phase = PromptPhase::Verifying;
        self.reconcile();
        true
    }

    /// Dismisses the shown prompt, advances the queue, and returns a watcher command if active.
    pub(crate) fn dismiss_shown(&mut self) -> Option<BannerCommand> {
        let dismissed = self.shown.take()?;
        self.reconcile();
        dismissed
            .prompt
            .phase
            .is_active()
            .then_some(BannerCommand::Dismiss {
                id: dismissed.prompt.request_id,
            })
    }

    /// Rotates the shown active prompt to the back of waiting prompts in the same priority tier.
    pub(crate) fn cycle_next(&mut self) -> bool {
        let Some(mut previous) = self.shown.take() else {
            return false;
        };
        let next_idx = previous
            .prompt
            .phase
            .is_active()
            .then(|| self.oldest_waiting_with_priority(previous.prompt.phase.priority()))
            .flatten();
        let Some(next_idx) = next_idx else {
            self.shown = Some(previous);
            return false;
        };
        let next = self.waiting.remove(next_idx);
        previous.order = self.take_order();
        self.waiting.push(previous);
        self.shown = Some(next);
        true
    }

    fn oldest_waiting_with_priority(&self, priority: u8) -> Option<usize> {
        self.waiting
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.prompt.phase.priority() == priority)
            .min_by_key(|(_, entry)| entry.order)
            .map(|(index, _)| index)
    }

    fn raise_fresh(&mut self, request_id: String, host: String, phase: PromptPhase) {
        self.clear_failed_for_host(&host, &request_id);
        self.raise(CloudflarePrompt {
            request_id,
            host,
            phase,
        });
    }

    fn take_order(&mut self) -> u64 {
        self.next_order += 1;
        self.next_order
    }

    fn clear_failed_for_host(&mut self, host: &str, keep_id: &str) {
        let is_stale_failure = |prompt: &CloudflarePrompt| {
            prompt.request_id != keep_id
                && prompt.host.eq_ignore_ascii_case(host)
                && matches!(prompt.phase, PromptPhase::Failed { .. })
        };
        if self
            .shown
            .as_ref()
            .is_some_and(|shown| is_stale_failure(&shown.prompt))
        {
            self.shown = None;
        }
        self.waiting
            .retain(|entry| !is_stale_failure(&entry.prompt));
    }

    fn settle_passed(&mut self, host: String, id: String, now: Instant) {
        if let Some(shown) = self
            .shown
            .as_mut()
            .filter(|shown| shown.prompt.request_id == id)
        {
            shown.prompt.host = host;
            shown.prompt.phase = PromptPhase::Verified {
                until: now + VERIFIED_SHOWN,
            };
        } else {
            self.waiting.retain(|entry| entry.prompt.request_id != id);
        }
    }

    fn remove_id(&mut self, id: &str) {
        if self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.prompt.request_id == id)
        {
            self.shown = None;
        } else {
            self.waiting.retain(|entry| entry.prompt.request_id != id);
        }
    }

    fn raise(&mut self, prompt: CloudflarePrompt) {
        if let Some(shown) = self
            .shown
            .as_mut()
            .filter(|shown| shown.prompt.request_id == prompt.request_id)
        {
            shown.prompt = prompt;
        } else if let Some(entry) = self
            .waiting
            .iter_mut()
            .find(|entry| entry.prompt.request_id == prompt.request_id)
        {
            entry.prompt = prompt;
        } else {
            let order = self.take_order();
            self.waiting.push(QueuedPrompt { prompt, order });
        }
    }

    fn best_waiting(&self) -> Option<(usize, &PromptPhase)> {
        self.waiting
            .iter()
            .enumerate()
            .min_by_key(|(_, entry)| (entry.prompt.phase.priority(), entry.order))
            .map(|(index, entry)| (index, &entry.prompt.phase))
    }

    fn reconcile(&mut self) {
        let Some((best_idx, best_phase)) = self.best_waiting() else {
            return;
        };
        if self
            .shown
            .as_ref()
            .is_some_and(|shown| !shown.prompt.phase.yields_to(best_phase))
        {
            return;
        }
        let next = self.waiting.remove(best_idx);
        if let Some(previous) = self.shown.replace(next) {
            if previous.prompt.phase.is_active() {
                self.waiting.push(previous);
            }
        }
    }
}

#[cfg(test)]
#[path = "cloudflare_prompts_tests.rs"]
mod tests;
