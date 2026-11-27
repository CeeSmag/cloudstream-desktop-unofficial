//! Downloads, normalizes, and delivers external subtitle files for the open video.
//!
//! Preferred languages are queued first (up to [`super::subtitle_budget::MAX_PREFERRED_EXTERNAL`]),
//! followed by other languages up to [`super::subtitle_budget::MAX_OTHER_EXTERNAL`] (at most
//! [`super::subtitle_budget::MAX_PER_OTHER_LANGUAGE`] per language). Failed downloads refund their
//! budget slot and automatically promote reserve tracks from healthy hosts.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use core_model::SubtitleTrack;
use player_api::Language;
use subtitle_store::{
    PrefetchCallbacks, PrefetchConfig, ReadySubtitle, SubtitleLoader, SubtitlePrefetch,
};

use crate::background_work::UiSender;
use crate::request_tickets::Ticket;
use crate::ui_events::{ReadySubtitleEvent, UiEvent};

use super::subtitle_budget::{subtitle_rank, QueueBudget};

/// Parallel subtitle download workers; time is spent waiting on remote subtitle hosts.
const DOWNLOADS_AT_ONCE: usize = 8;

/// Snapshot of background subtitle download progress for the player UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SubtitleLoadSnapshot {
    pub(crate) submitted: usize,
    pub(crate) ready: usize,
    pub(crate) failed: usize,
    pub(crate) busy: bool,
}

/// Parameters to start subtitle delivery for one open or refresh ticket.
pub(crate) struct DeliveryRequest {
    pub(crate) loader: Arc<SubtitleLoader>,
    pub(crate) title_key: String,
    pub(crate) preferred: Vec<Language>,
    pub(crate) already_delivered: HashSet<String>,
    pub(crate) ticket: Ticket,
    pub(crate) sender: UiSender,
}

/// Manages background subtitle downloads for the currently open video.
pub(crate) struct SubtitleDeliveryHandle {
    active: Arc<AtomicBool>,
    prefetch: Arc<SubtitlePrefetch>,
    budget: Arc<Mutex<QueueBudget>>,
}

impl SubtitleDeliveryHandle {
    /// Offers cached subtitles for the title immediately and starts background prefetch workers.
    pub(crate) fn start(mut request: DeliveryRequest) -> Self {
        let mut initial_budget = QueueBudget::new(
            request.preferred.clone(),
            std::mem::take(&mut request.already_delivered),
        );
        offer_cached(&request, &mut initial_budget);

        let budget = Arc::new(Mutex::new(initial_budget));
        let active = Arc::new(AtomicBool::new(true));
        let prefetch_slot: Arc<OnceLock<Weak<SubtitlePrefetch>>> = Arc::new(OnceLock::new());
        let rank_languages = request.preferred.clone();
        let config = PrefetchConfig {
            workers: DOWNLOADS_AT_ONCE,
            rank_of: Arc::new(move |label| subtitle_rank(&rank_languages, label)),
        };
        let callbacks = build_callbacks(
            &request,
            Arc::clone(&active),
            Arc::clone(&budget),
            Arc::clone(&prefetch_slot),
        );
        let (prefetch, _handles) =
            SubtitlePrefetch::start_with_callbacks(request.loader, &config, callbacks);
        let prefetch = Arc::new(prefetch);
        let _ = prefetch_slot.set(Arc::downgrade(&prefetch));
        Self {
            active,
            prefetch,
            budget,
        }
    }

    /// Queues newly discovered provider subtitle tracks, deduplicated and capped by budget.
    pub(crate) fn submit(&self, tracks: &[SubtitleTrack]) {
        if !self.active.load(Ordering::SeqCst) || tracks.is_empty() {
            return;
        }
        let accepted = {
            let mut budget = self.budget.lock().unwrap_or_else(PoisonError::into_inner);
            budget.filter_accepted(tracks)
        };
        if !accepted.is_empty() {
            self.prefetch.submit(accepted);
        }
    }

    /// Pauses or resumes pulling new subtitle downloads (used when Sources Focus is active).
    pub(crate) fn set_paused(&self, paused: bool) {
        self.prefetch.set_paused(paused);
    }

    /// True once preferred languages (or any language when none configured) have enough ready files.
    pub(crate) fn is_preferred_satisfied(&self) -> bool {
        self.budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_preferred_satisfied()
    }

    /// No more tracks will be discovered for this open/refresh; workers exit once the queue drains.
    pub(crate) fn finish_discovery(&self) {
        self.prefetch.finish();
    }

    /// Cancels queued downloads immediately and suppresses late deliveries.
    pub(crate) fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
        self.prefetch.stop();
    }

    /// Current download progress snapshot for the loading indicator.
    pub(crate) fn snapshot(&self) -> SubtitleLoadSnapshot {
        let summary = self.prefetch.summary();
        SubtitleLoadSnapshot {
            submitted: summary.submitted,
            ready: summary.ready,
            failed: summary.failed,
            busy: summary.queued > 0 || summary.in_flight > 0,
        }
    }
}

fn build_callbacks(
    request: &DeliveryRequest,
    active: Arc<AtomicBool>,
    budget: Arc<Mutex<QueueBudget>>,
    prefetch_slot: Arc<OnceLock<Weak<SubtitlePrefetch>>>,
) -> PrefetchCallbacks {
    let keep_flag = Arc::clone(&active);
    let ready_budget = Arc::clone(&budget);
    let target = DeliveryTarget {
        active: Arc::clone(&active),
        loader: Arc::clone(&request.loader),
        title_key: request.title_key.clone(),
        ticket: request.ticket,
        sender: request.sender.clone(),
    };
    let on_ready = Arc::new(move |ready: ReadySubtitle| {
        if target.active.load(Ordering::SeqCst) {
            ready_budget
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .on_ready(&ready.language);
        }
        target.deliver(ready);
    });
    let on_failed = Arc::new(move |track: &SubtitleTrack, _reason: &str| {
        if !active.load(Ordering::SeqCst) {
            return;
        }
        let promoted = budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .on_failed(track);
        if !promoted.is_empty() {
            if let Some(prefetch) = prefetch_slot.get().and_then(Weak::upgrade) {
                prefetch.submit(promoted);
            }
        }
    });
    PrefetchCallbacks {
        on_ready,
        on_failed,
        keep_going: Arc::new(move || keep_flag.load(Ordering::SeqCst)),
    }
}

fn offer_cached(request: &DeliveryRequest, budget: &mut QueueBudget) {
    let mut cached = request.loader.cache().cached_for_title(&request.title_key);
    if cached.is_empty() {
        return;
    }
    cached.sort_by_key(|sub| subtitle_rank(&request.preferred, &sub.language));
    tracing::info!(
        files = cached.len(),
        "offering cached subtitles for this title immediately"
    );
    for ready in cached {
        let probe = SubtitleTrack {
            lang: ready.language.clone(),
            url: ready.url.clone(),
            headers: None,
        };
        if budget.accept_cached(&probe) {
            request
                .sender
                .send(UiEvent::SubtitleReady(request.ticket, ready_event(&ready)));
        }
    }
}

struct DeliveryTarget {
    active: Arc<AtomicBool>,
    loader: Arc<SubtitleLoader>,
    title_key: String,
    ticket: Ticket,
    sender: UiSender,
}

impl DeliveryTarget {
    fn deliver(&self, ready: ReadySubtitle) {
        if !self.active.load(Ordering::SeqCst) {
            self.loader
                .cache()
                .discard_unclaimed(&self.title_key, &ready);
            return;
        }
        if let Err(error) = self
            .loader
            .cache()
            .remember_for_title(&self.title_key, &ready)
        {
            tracing::warn!(%error, "could not record subtitle in the title's saved list");
        }
        self.sender
            .send(UiEvent::SubtitleReady(self.ticket, ready_event(&ready)));
    }
}

fn ready_event(ready: &ReadySubtitle) -> ReadySubtitleEvent {
    ReadySubtitleEvent {
        remote_url: ready.url.clone(),
        track: SubtitleTrack {
            lang: ready.language.clone(),
            url: ready.path.to_string_lossy().replace('\\', "/"),
            headers: None,
        },
    }
}

#[cfg(test)]
#[path = "subtitle_delivery_tests.rs"]
mod tests;
