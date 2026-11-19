//! Search/check loop for a stream open (best working after full scan, skip, or time cap).

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use app_services::{LinkHealth, LinkSearch, StreamChecker};
use core_model::{PlayableLink, SubtitleTrack};

use crate::open_gate::{
    best_after_working, no_streams_timed_out, should_announce_ready, ReadyWhen,
};
use crate::open_types::{CheckedOpen, OpenError, OpenHooks, OpenProgress};

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

    fn timed_out(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(TOTAL_SECS)
    }

    fn all_checked(&self) -> bool {
        self.next_to_check >= self.ordered.len()
    }

    pub(crate) fn announce_ready(&mut self, hooks: &OpenHooks<'_>) {
        if self.ready_announced {
            return;
        }
        let Some(index) = self.play_index else {
            return;
        };
        self.ready_announced = true;
        (hooks.on_ready)(index);
        (hooks.on_update)(&self.snapshot());
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

/// True when the open should stop polling the search.
fn should_stop_search(state: &OpenState, hooks: &OpenHooks<'_>) -> bool {
    let skipped = hooks.skip.load(Ordering::SeqCst);
    hooks.cancel.load(Ordering::SeqCst)
        || state.timed_out()
        || (skipped && state.play_index.is_some())
        || no_streams_timed_out(
            state.ordered.len(),
            state.started.elapsed().as_secs(),
            FIRST_WORKING_SECS,
        )
}

pub(crate) fn run_search(
    mut search: LinkSearch,
    mut state: OpenState,
    checker: &StreamChecker,
    hooks: &OpenHooks<'_>,
) -> Result<CheckedOpen, OpenError> {
    while !should_stop_search(&state, hooks) {
        let batch = search
            .poll(Duration::from_millis(200))
            .map_err(|error| OpenError::Search(error.to_string()))?;
        if let Some(failure) = &batch.failure {
            if state.ordered.is_empty() {
                search.cancel();
                return Err(OpenError::Search(failure.clone()));
            }
        }
        absorb_batch(&mut state, batch.links, batch.subtitles, hooks);
        check_pending(&mut state, checker, hooks);
        state.maybe_announce_ready(hooks);
        if batch.done {
            state.search_done = true;
            (hooks.on_progress)(state.progress());
            check_pending(&mut state, checker, hooks);
            state.maybe_announce_ready(hooks);
            break;
        }
    }
    if state.ordered.is_empty()
        && no_streams_timed_out(
            0,
            state.started.elapsed().as_secs(),
            FIRST_WORKING_SECS,
        )
    {
        search.cancel();
        return Err(OpenError::Empty);
    }
    state.maybe_announce_ready(hooks);
    if state.timed_out() && !state.ready_announced {
        check_pending(&mut state, checker, hooks);
        state.maybe_announce_ready(hooks);
    }
    search.cancel();
    finish(state)
}

fn absorb_batch(
    state: &mut OpenState,
    links: Vec<PlayableLink>,
    tracks: Vec<SubtitleTrack>,
    hooks: &OpenHooks<'_>,
) {
    let mut added = false;
    for link in links {
        if state.ordered.iter().any(|(known, _)| known.url == link.url) {
            continue;
        }
        state.ordered.push((link, None));
        added = true;
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

pub(crate) fn check_pending(state: &mut OpenState, checker: &StreamChecker, hooks: &OpenHooks<'_>) {
    while state.next_to_check < state.ordered.len() {
        if hooks.cancel.load(Ordering::SeqCst) {
            break;
        }
        if hooks.skip.load(Ordering::SeqCst) && state.play_index.is_some() {
            break;
        }
        if state.timed_out() && state.play_index.is_some() {
            break;
        }
        check_one(state, checker, hooks);
        state.maybe_announce_ready(hooks);
        if state.ready_announced && hooks.skip.load(Ordering::SeqCst) {
            break;
        }
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

pub(crate) fn finish(state: OpenState) -> Result<CheckedOpen, OpenError> {
    if state.ordered.is_empty() {
        return Err(OpenError::Empty);
    }
    Ok(state.snapshot())
}

#[cfg(test)]
#[path = "open_run_tests.rs"]
mod tests;
