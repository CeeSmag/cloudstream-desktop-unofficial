//! Downloads, normalizes, and delivers external subtitle files for the open video.
//!
//! Preferred languages are queued first (up to [`MAX_PREFERRED_EXTERNAL`]), followed by other
//! languages up to [`MAX_OTHER_EXTERNAL`] (at most [`MAX_PER_OTHER_LANGUAGE`] per language) so
//! desired subtitles arrive fast without hiding alternate languages or flooding mpv.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use core_model::SubtitleTrack;
use player_api::{find_language, Language};
use subtitle_store::{PrefetchConfig, ReadySubtitle, SubtitleLoader, SubtitlePrefetch};

use crate::background_work::UiSender;
use crate::request_tickets::Ticket;
use crate::ui_events::{ReadySubtitleEvent, UiEvent};

/// Parallel subtitle download workers; time is spent waiting on remote subtitle hosts.
const DOWNLOADS_AT_ONCE: usize = 8;
/// Max external subtitle files queued in the viewer's preferred languages.
const MAX_PREFERRED_EXTERNAL: usize = 12;
/// Max external subtitle files queued in non-preferred languages.
const MAX_OTHER_EXTERNAL: usize = 12;
/// Max files per single non-preferred language so duplicates do not crowd out other languages.
const MAX_PER_OTHER_LANGUAGE: usize = 2;

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
    prefetch: SubtitlePrefetch,
    budget: Mutex<QueueBudget>,
}

impl SubtitleDeliveryHandle {
    /// Offers cached subtitles for the title immediately and starts background prefetch workers.
    pub(crate) fn start(mut request: DeliveryRequest) -> Self {
        let mut budget = QueueBudget::new(
            request.preferred.clone(),
            std::mem::take(&mut request.already_delivered),
        );
        offer_cached(&request, &mut budget);

        let active = Arc::new(AtomicBool::new(true));
        let rank_languages = request.preferred.clone();
        let config = PrefetchConfig {
            workers: DOWNLOADS_AT_ONCE,
            rank_of: Arc::new(move |label| subtitle_rank(&rank_languages, label)),
        };
        let keep_flag = Arc::clone(&active);
        let target = DeliveryTarget {
            active: Arc::clone(&active),
            loader: Arc::clone(&request.loader),
            title_key: request.title_key,
            ticket: request.ticket,
            sender: request.sender,
        };
        let (prefetch, _handles) = SubtitlePrefetch::start(
            request.loader,
            &config,
            Arc::new(move |ready| target.deliver(ready)),
            Arc::new(move || keep_flag.load(Ordering::SeqCst)),
        );
        Self {
            active,
            prefetch,
            budget: Mutex::new(budget),
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
        if budget.accept(&probe) {
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

/// Tracks which remote URLs have been accepted and enforces preferred / non-preferred caps.
struct QueueBudget {
    preferred: Vec<Language>,
    seen_urls: HashSet<String>,
    preferred_count: usize,
    other_count: usize,
    other_per_lang: HashMap<String, usize>,
}

impl QueueBudget {
    fn new(preferred: Vec<Language>, already_delivered: HashSet<String>) -> Self {
        Self {
            preferred,
            seen_urls: already_delivered,
            preferred_count: 0,
            other_count: 0,
            other_per_lang: HashMap::new(),
        }
    }

    fn filter_accepted(&mut self, tracks: &[SubtitleTrack]) -> Vec<SubtitleTrack> {
        let mut ordered: Vec<&SubtitleTrack> = tracks.iter().collect();
        ordered.sort_by_key(|track| subtitle_rank(&self.preferred, &track.lang));
        ordered
            .into_iter()
            .filter(|track| self.accept(track))
            .cloned()
            .collect()
    }

    fn accept(&mut self, track: &SubtitleTrack) -> bool {
        if track.url.trim().is_empty() || self.seen_urls.contains(&track.url) {
            return false;
        }
        let is_preferred = !self.preferred.is_empty()
            && subtitle_rank(&self.preferred, &track.lang) < self.preferred.len();
        if is_preferred {
            if self.preferred_count >= MAX_PREFERRED_EXTERNAL {
                return false;
            }
            self.preferred_count += 1;
        } else {
            let cap = if self.preferred.is_empty() {
                MAX_PREFERRED_EXTERNAL + MAX_OTHER_EXTERNAL
            } else {
                MAX_OTHER_EXTERNAL
            };
            let bucket = language_bucket(&track.lang);
            let per_lang = self.other_per_lang.get(&bucket).copied().unwrap_or(0);
            if self.other_count >= cap || per_lang >= MAX_PER_OTHER_LANGUAGE {
                return false;
            }
            self.other_count += 1;
            self.other_per_lang.insert(bucket, per_lang + 1);
        }
        self.seen_urls.insert(track.url.clone());
        true
    }
}

/// True when `label` (`"en"`, `"eng"`, `"en-US"`, `"English"`, `"English (SDH)"`) names `language`.
pub(crate) fn matches_subtitle_label(label: &str, language: &Language) -> bool {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return false;
    }
    if language.has_code(trimmed) {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    let name = language.name.to_ascii_lowercase();
    lower == name
        || lower
            .strip_prefix(&name)
            .is_some_and(|rest| rest.starts_with([' ', '-', '(', '[', '_', ':', '.']))
}

/// Lower rank downloads first: preferred languages get `0..preferred.len()`, others get `preferred.len()`.
pub(crate) fn subtitle_rank(preferred: &[Language], label: &str) -> usize {
    preferred
        .iter()
        .position(|language| matches_subtitle_label(label, language))
        .unwrap_or(preferred.len())
}

fn language_bucket(label: &str) -> String {
    let trimmed = label.trim();
    if let Some(language) = find_language(trimmed) {
        return language.code().to_owned();
    }
    trimmed
        .split([' ', '-', '(', '['])
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
}

#[cfg(test)]
#[path = "subtitle_delivery_tests.rs"]
mod tests;
