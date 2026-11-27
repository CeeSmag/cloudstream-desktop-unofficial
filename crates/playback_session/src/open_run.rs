//! Search/check loop for a stream open (best working after full scan, skip, or time cap).

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[cfg(test)]
use app_services::LinkSearch;
use app_services::{LinkHealth, StreamChecker};
use core_model::{PlayableLink, SubtitleTrack};

#[cfg(test)]
use crate::check_pool::{run_search_pool, PoolConfig};
use crate::open_gate::{best_after_working, should_announce_ready, ReadyWhen};
use crate::open_types::{CheckedOpen, OpenError, OpenHooks, OpenProgress};
#[cfg(test)]
use crate::subtitle_priority::CheckGate;

/// Cap how long we wait with no streams at all before failing the open.
pub(crate) const FIRST_WORKING_SECS: u64 = 30;
/// Cap for the whole search + check pass; then start the best working so far.
pub(crate) const TOTAL_SECS: u64 = 60;

pub(crate) struct OpenState {
    pub(crate) ordered: Vec<(PlayableLink, Option<LinkHealth>)>,
    pub(crate) subtitles: Vec<SubtitleTrack>,
    pub(crate) checked: usize,
    pub(crate) working: usize,
    pub(crate) play_index: Option<usize>,
    pub(crate) next_to_check: usize,
    pub(crate) started: Instant,
    pub(crate) search_done: bool,
    pub(crate) ready_announced: bool,
}

impl OpenState {
    pub(crate) fn new() -> Self {
        Self {
            ordered: Vec::new(),
            subtitles: Vec::new(),
            checked: 0,
            working: 0,
            play_index: None,
            next_to_check: 0,
            started: Instant::now(),
            search_done: false,
            ready_announced: false,
        }
    }

    pub(crate) fn progress(&self) -> OpenProgress {
        OpenProgress {
            found: self.ordered.len(),
            checked: self.checked,
            working: self.working,
            search_done: self.search_done,
        }
    }

    pub(crate) fn snapshot(&self) -> CheckedOpen {
        CheckedOpen {
            links: self.ordered.clone(),
            play_index: self.play_index,
            subtitles: self.subtitles.clone(),
        }
    }

    pub(crate) fn timed_out(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(TOTAL_SECS)
    }

    pub(crate) fn all_checked(&self) -> bool {
        self.ordered.iter().all(|(_, health)| health.is_some())
    }

    pub(crate) fn announce_ready(&mut self, hooks: &OpenHooks<'_>) {
        if self.ready_announced {
            return;
        }
        let Some(index) = self.play_index else {
            return;
        };
        self.ready_announced = true;
        (hooks.on_update)(&self.snapshot());
        (hooks.on_ready)(index);
    }

    /// Starts when skip, total cap, or search+checks are complete (and something works).
    pub(crate) fn maybe_announce_ready(&mut self, hooks: &OpenHooks<'_>) {
        let skip = hooks.skip.load(Ordering::SeqCst);
        if should_announce_ready(ReadyWhen {
            has_working: self.play_index.is_some(),
            search_done: self.search_done,
            all_checked: self.all_checked(),
            skip,
            total_timed_out: self.timed_out(),
        }) {
            self.announce_ready(hooks);
        }
    }
}

#[cfg(test)]
pub(crate) fn run_search(
    search: LinkSearch,
    state: OpenState,
    checker: &StreamChecker,
    hooks: &OpenHooks<'_>,
) -> Result<CheckedOpen, OpenError> {
    let gate = CheckGate::default();
    let config = PoolConfig::standard(checker, hooks, &gate);
    run_search_pool(search, state, &config)
}

pub(crate) fn check_pending(state: &mut OpenState, checker: &StreamChecker, hooks: &OpenHooks<'_>) {
    while state.next_to_check < state.ordered.len() {
        if hooks.cancel.load(Ordering::SeqCst) {
            break;
        }
        if state.timed_out() && state.play_index.is_some() {
            break;
        }
        check_one(state, checker, hooks);
        state.maybe_announce_ready(hooks);
    }
}

pub(crate) fn check_one(state: &mut OpenState, checker: &StreamChecker, hooks: &OpenHooks<'_>) {
    let index = state.next_to_check;
    let Some((link, slot)) = state.ordered.get_mut(index) else {
        return;
    };
    let health = checker.check(link);
    let is_ok = health.is_working();
    *slot = Some(health);
    state.checked += 1;
    if is_ok {
        state.working += 1;
        state.play_index = Some(best_after_working(state.play_index, index, &state.ordered));
    }
    state.next_to_check += 1;
    (hooks.on_progress)(state.progress());
    (hooks.on_update)(&state.snapshot());
}

#[cfg(test)]
pub(crate) fn absorb_batch(
    state: &mut OpenState,
    links: Vec<PlayableLink>,
    tracks: Vec<SubtitleTrack>,
    hooks: &OpenHooks<'_>,
) {
    let mut added = false;
    for link in links {
        if !state.ordered.iter().any(|(known, _)| known.url == link.url) {
            state.ordered.push((link, None));
            added = true;
        }
    }
    for track in tracks {
        if !state.subtitles.iter().any(|known| known.url == track.url) {
            state.subtitles.push(track);
            added = true;
        }
    }
    if added {
        (hooks.on_progress)(state.progress());
        (hooks.on_update)(&state.snapshot());
    }
}

pub(crate) fn finish(state: OpenState) -> Result<CheckedOpen, OpenError> {
    if state.ordered.is_empty() {
        return Err(OpenError::Empty);
    }
    Ok(state.snapshot())
}

#[cfg(test)]
#[path = "open_run_tests.rs"]
mod tests;
