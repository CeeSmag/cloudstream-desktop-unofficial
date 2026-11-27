//! Pure subtitle download budget, failure refunding, reserve promotion, and preferred-language
//! satisfaction tracking.

use std::collections::{HashMap, HashSet};

use core_model::SubtitleTrack;
use player_api::{find_language, Language};
use subtitle_store::host_of;

/// Max active (in-flight + ready) external subtitle files in the viewer's preferred languages.
pub const MAX_PREFERRED_EXTERNAL: usize = 12;
/// Max active (in-flight + ready) external subtitle files in non-preferred languages.
pub const MAX_OTHER_EXTERNAL: usize = 12;
/// Max active files per single non-preferred language so duplicates do not crowd out other languages.
pub const MAX_PER_OTHER_LANGUAGE: usize = 2;
/// Bounded reserve backlog of overflow tracks eligible for promotion when an active download fails.
pub const MAX_RESERVE_TRACKS: usize = 96;
/// Working ready subtitles required per preferred language (or total when none configured) before
/// automatic post-Play-now subtitle priority yields back to stream checking.
pub const PREFERRED_READY_PER_LANGUAGE: usize = 2;

/// Tracks accepted subtitle tracks, refunds failed downloads, promotes reserve backups from healthy
/// hosts, and reports when preferred subtitle languages have enough working tracks.
pub struct QueueBudget {
    preferred: Vec<Language>,
    seen_urls: HashSet<String>,
    active_urls: HashSet<String>,
    preferred_count: usize,
    other_count: usize,
    other_per_lang: HashMap<String, usize>,
    preferred_ready_by_slot: Vec<usize>,
    ready_count: usize,
    failed_hosts: HashSet<String>,
    reserve: Vec<SubtitleTrack>,
}

impl QueueBudget {
    /// Creates a budget for one video open/refresh, skipping `already_delivered` URLs.
    pub fn new(preferred: Vec<Language>, already_delivered: HashSet<String>) -> Self {
        let slot_count = preferred.len();
        Self {
            preferred,
            seen_urls: already_delivered,
            active_urls: HashSet::new(),
            preferred_count: 0,
            other_count: 0,
            other_per_lang: HashMap::new(),
            preferred_ready_by_slot: vec![0; slot_count],
            ready_count: 0,
            failed_hosts: HashSet::new(),
            reserve: Vec::new(),
        }
    }

    /// Accepts a cached ready subtitle into the budget and counts it as already ready.
    pub fn accept_cached(&mut self, track: &SubtitleTrack) -> bool {
        if !self.accept(track) {
            return false;
        }
        self.on_ready(&track.lang);
        true
    }

    /// Accepts newly discovered tracks up to the budget caps; overflow tracks go into the reserve
    /// backlog so failed downloads can be replaced automatically.
    pub fn filter_accepted(&mut self, tracks: &[SubtitleTrack]) -> Vec<SubtitleTrack> {
        let mut ordered: Vec<&SubtitleTrack> = tracks.iter().collect();
        ordered.sort_by_key(|track| {
            (
                self.is_failed_host(&track.url),
                subtitle_rank(&self.preferred, &track.lang),
            )
        });
        let mut accepted = Vec::new();
        for track in ordered {
            if track.url.trim().is_empty() || self.seen_urls.contains(&track.url) {
                continue;
            }
            if self.try_claim_slot(track) {
                self.seen_urls.insert(track.url.clone());
                self.active_urls.insert(track.url.clone());
                accepted.push(track.clone());
            } else if self.reserve.len() < MAX_RESERVE_TRACKS {
                self.seen_urls.insert(track.url.clone());
                self.reserve.push(track.clone());
            }
        }
        accepted
    }

    /// Records a working ready subtitle for preferred-language satisfaction tracking.
    pub fn on_ready(&mut self, language: &str) {
        self.ready_count += 1;
        let rank = subtitle_rank(&self.preferred, language);
        if let Some(slot) = self.preferred_ready_by_slot.get_mut(rank) {
            *slot += 1;
        }
    }

    /// Refunds the budget slot of a failed subtitle download, marks its host as degraded, and
    /// promotes the best eligible reserve track (preferring healthy hosts).
    pub fn on_failed(&mut self, track: &SubtitleTrack) -> Vec<SubtitleTrack> {
        if let Some(host) = host_of(&track.url) {
            self.failed_hosts.insert(host);
        }
        if self.active_urls.remove(&track.url) {
            self.release_slot(track);
        }
        self.promote_from_reserve()
    }

    /// True once at least [`PREFERRED_READY_PER_LANGUAGE`] working subtitles are ready for every
    /// configured preferred language (or total when no preferred languages are set).
    pub fn is_preferred_satisfied(&self) -> bool {
        if self.preferred.is_empty() {
            self.ready_count >= PREFERRED_READY_PER_LANGUAGE
        } else {
            self.preferred_ready_by_slot
                .iter()
                .all(|&count| count >= PREFERRED_READY_PER_LANGUAGE)
        }
    }

    fn accept(&mut self, track: &SubtitleTrack) -> bool {
        if track.url.trim().is_empty() || self.seen_urls.contains(&track.url) {
            return false;
        }
        if !self.try_claim_slot(track) {
            return false;
        }
        self.seen_urls.insert(track.url.clone());
        self.active_urls.insert(track.url.clone());
        true
    }

    fn is_preferred(&self, label: &str) -> bool {
        !self.preferred.is_empty() && subtitle_rank(&self.preferred, label) < self.preferred.len()
    }

    fn other_cap(&self) -> usize {
        if self.preferred.is_empty() {
            MAX_PREFERRED_EXTERNAL + MAX_OTHER_EXTERNAL
        } else {
            MAX_OTHER_EXTERNAL
        }
    }

    fn try_claim_slot(&mut self, track: &SubtitleTrack) -> bool {
        if self.is_preferred(&track.lang) {
            if self.preferred_count >= MAX_PREFERRED_EXTERNAL {
                return false;
            }
            self.preferred_count += 1;
            return true;
        }
        let bucket = language_bucket(&track.lang);
        let per_lang = self.other_per_lang.get(&bucket).copied().unwrap_or(0);
        if self.other_count >= self.other_cap() || per_lang >= MAX_PER_OTHER_LANGUAGE {
            return false;
        }
        self.other_count += 1;
        self.other_per_lang.insert(bucket, per_lang + 1);
        true
    }

    fn release_slot(&mut self, track: &SubtitleTrack) {
        if self.is_preferred(&track.lang) {
            self.preferred_count = self.preferred_count.saturating_sub(1);
            return;
        }
        self.other_count = self.other_count.saturating_sub(1);
        let bucket = language_bucket(&track.lang);
        if let Some(count) = self.other_per_lang.get_mut(&bucket) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.other_per_lang.remove(&bucket);
            }
        }
    }

    fn promote_from_reserve(&mut self) -> Vec<SubtitleTrack> {
        let mut promoted = Vec::new();
        while let Some(index) = self.pick_best_reserve_index() {
            let candidate = self.reserve.remove(index);
            if self.try_claim_slot(&candidate) {
                self.active_urls.insert(candidate.url.clone());
                promoted.push(candidate);
            }
        }
        promoted
    }

    fn pick_best_reserve_index(&self) -> Option<usize> {
        let mut best: Option<(usize, (bool, usize))> = None;
        for (index, track) in self.reserve.iter().enumerate() {
            if !self.can_claim_slot(track) {
                continue;
            }
            let key = (
                self.is_failed_host(&track.url),
                subtitle_rank(&self.preferred, &track.lang),
            );
            if best.is_none_or(|(_, best_key)| key < best_key) {
                best = Some((index, key));
            }
        }
        best.map(|(index, _)| index)
    }

    fn can_claim_slot(&self, track: &SubtitleTrack) -> bool {
        if self.is_preferred(&track.lang) {
            return self.preferred_count < MAX_PREFERRED_EXTERNAL;
        }
        let bucket = language_bucket(&track.lang);
        let per_lang = self.other_per_lang.get(&bucket).copied().unwrap_or(0);
        self.other_count < self.other_cap() && per_lang < MAX_PER_OTHER_LANGUAGE
    }

    fn is_failed_host(&self, url: &str) -> bool {
        host_of(url).is_some_and(|host| self.failed_hosts.contains(&host))
    }
}

/// True when `label` (`"en"`, `"eng"`, `"en-US"`, `"English"`, `"English (SDH)"`) names `language`.
pub fn matches_subtitle_label(label: &str, language: &Language) -> bool {
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
pub fn subtitle_rank(preferred: &[Language], label: &str) -> usize {
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
#[path = "subtitle_budget_tests.rs"]
mod tests;
