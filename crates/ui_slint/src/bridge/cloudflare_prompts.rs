//! Which Cloudflare check the banner shows when several sites ask at once.
//!
//! The banner has room for one site. Everything the watcher reports goes through [`PromptQueue`], which
//! keeps the one on screen and the ones waiting behind it, so a burst of sites no longer overwrites
//! each other and a request for the viewer's permission is never lost behind a check that merely runs.

use cloudflare_bridge::CloudflareEvent;

/// The banner's look: state (see `CloudflareBanner`), site and reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudflareBannerView {
    pub(crate) state: i32,
    pub(crate) host: String,
    pub(crate) message: String,
    pub(crate) request_id: String,
    pub(crate) can_retry: bool,
}

/// The banner for `event`; state 0 means the request is gone.
pub(crate) fn banner_for(event: CloudflareEvent) -> Option<CloudflareBannerView> {
    let (state, host, message, request_id, can_retry) = match event {
        CloudflareEvent::NeedsPermission { host, id } => (1, host, String::new(), id, false),
        CloudflareEvent::Verifying { host, id } => (2, host, String::new(), id, false),
        CloudflareEvent::Passed { host, id } => (3, host, String::new(), id, false),
        CloudflareEvent::Failed {
            host,
            id,
            reason,
            retry,
        } => (4, host, reason, id, retry),
        CloudflareEvent::Dismissed { id } => (0, String::new(), String::new(), id, false),
        CloudflareEvent::PermissionGranted { .. } => return None,
    };
    Some(CloudflareBannerView {
        state,
        host,
        message,
        request_id,
        can_retry,
    })
}

struct Waiting {
    view: CloudflareBannerView,
    order: u64,
}

/// The prompt on screen and the prompts waiting for their turn.
#[derive(Default)]
pub(crate) struct PromptQueue {
    shown: Option<CloudflareBannerView>,
    waiting: Vec<Waiting>,
    arrivals: u64,
}

/// Needs permission first (the viewer can act on it), then a failure to retry, then a check that runs.
fn rank(state: i32) -> u8 {
    match state {
        1 => 0,
        4 => 1,
        _ => 2,
    }
}

impl PromptQueue {
    pub(crate) fn shown(&self) -> Option<&CloudflareBannerView> {
        self.shown.as_ref()
    }

    /// How many more sites wait behind the one on screen.
    pub(crate) fn more(&self) -> usize {
        self.waiting.len()
    }

    /// Takes one report from the watcher.
    pub(crate) fn apply(&mut self, view: CloudflareBannerView) {
        match view.state {
            0 | 3 => self.settle(view),
            _ => self.raise(view),
        }
        self.fill_screen();
    }

    /// The viewer dismissed the prompt on screen, or its "verified" time is up.
    pub(crate) fn finish_shown(&mut self, id: &str) {
        if self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.request_id == id)
        {
            self.shown = None;
            self.fill_screen();
        }
    }

    /// The viewer started a fresh attempt of the failed check on screen, under a new request.
    pub(crate) fn retrying(&mut self, new_id: String) {
        if let Some(shown) = self.shown.as_mut() {
            shown.request_id = new_id;
            shown.state = 2;
            shown.can_retry = false;
            shown.message.clear();
        }
    }

    fn is_shown(&self, id: &str) -> bool {
        self.shown
            .as_ref()
            .is_some_and(|shown| shown.request_id == id)
    }

    /// The request is verified or gone. A prompt that never reached the screen is dropped silently.
    fn settle(&mut self, view: CloudflareBannerView) {
        if !self.is_shown(&view.request_id) {
            self.waiting
                .retain(|entry| entry.view.request_id != view.request_id);
        } else if view.state == 3 {
            self.shown = Some(view);
        } else {
            self.shown = None;
        }
    }

    fn raise(&mut self, view: CloudflareBannerView) {
        if self.is_shown(&view.request_id) {
            self.shown = Some(view);
        } else if let Some(entry) = self
            .waiting
            .iter_mut()
            .find(|entry| entry.view.request_id == view.request_id)
        {
            entry.view = view;
        } else {
            self.arrivals += 1;
            self.waiting.push(Waiting {
                view,
                order: self.arrivals,
            });
        }
    }

    fn best_waiting(&self) -> Option<usize> {
        (0..self.waiting.len()).min_by_key(|&index| {
            self.waiting
                .get(index)
                .map_or((u8::MAX, u64::MAX), |entry| {
                    (rank(entry.view.state), entry.order)
                })
        })
    }

    /// Puts the best waiting prompt on an empty screen. A request for permission also takes the screen
    /// from a check that only runs or has just passed, which goes back to waiting if it still runs.
    fn fill_screen(&mut self) {
        let Some(index) = self.best_waiting() else {
            return;
        };
        let best_state = self.waiting.get(index).map_or(0, |entry| entry.view.state);
        match self.shown.as_ref().map(|shown| shown.state) {
            None => {}
            Some(2 | 3) if best_state == 1 => {
                if let Some(shown) = self.shown.take().filter(|shown| shown.state == 2) {
                    self.arrivals += 1;
                    self.waiting.push(Waiting {
                        view: shown,
                        order: 0,
                    });
                }
            }
            Some(_) => return,
        }
        if let Some(index) = self.best_waiting() {
            self.shown = Some(self.waiting.remove(index).view);
        }
    }
}

#[cfg(test)]
#[path = "cloudflare_prompts_tests.rs"]
mod tests;
