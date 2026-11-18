//! Search/check loop and TitleCache seeding for a stream open.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use app_services::{LinkHealth, LinkSearch, StreamChecker};
use core_model::{PlayableLink, SubtitleTrack};
use title_cache::{Freshness, Lookup};

use crate::open_types::{CheckedOpen, OpenCache, OpenError, OpenHooks, OpenProgress};

/// Cap how long we wait for the first working stream before returning what we have.
pub(crate) const FIRST_WORKING_SECS: u64 = 30;
/// Cap for the whole search + check pass.
pub(crate) const TOTAL_SECS: u64 = 60;
/// How many saved streams to plant when the cache is fresh.
const TRUSTED_RECHECK: usize = 8;
/// How many saved streams to test first when the cache is stale.
const STALE_TEST_FIRST: usize = 3;
/// Cap for probing stale saved streams before falling through to the live search.
const STALE_TEST_SECS: u64 = 3;

pub(crate) struct OpenState {
    ordered: Vec<(PlayableLink, Option<LinkHealth>)>,
    subtitles: Vec<SubtitleTrack>,
    checked: usize,
    working: usize,
    play_index: Option<usize>,
    next_to_check: usize,
    started: Instant,
    search_done: bool,
    ready_announced: bool,
    trusted_played: bool,
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
            trusted_played: false,
        }
    }

    fn progress(&self) -> OpenProgress {
        OpenProgress {
            found: self.ordered.len(),
            checked: self.checked,
            working: self.working,
            search_done: self.search_done,
        }
    }

    fn snapshot(&self) -> CheckedOpen {
        CheckedOpen {
            links: self.ordered.clone(),
            play_index: self.play_index,
            subtitles: self.subtitles.clone(),
        }
    }

    fn timed_out(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(TOTAL_SECS)
    }

    fn first_working_wait_done(&self) -> bool {
        self.play_index.is_some()
            && self.started.elapsed() >= Duration::from_secs(FIRST_WORKING_SECS)
            && self.search_done
    }

    fn announce_ready(&mut self, hooks: &OpenHooks<'_>) {
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
}

pub(crate) fn plant_saved(
    state: &mut OpenState,
    open_cache: &OpenCache,
    checker: &StreamChecker,
    hooks: &OpenHooks<'_>,
) {
    let Lookup::Hit(hit) = open_cache.cache.load(&open_cache.title_key) else {
        return;
    };
    let take = match hit.freshness {
        Freshness::Fresh => TRUSTED_RECHECK,
        Freshness::Stale => STALE_TEST_FIRST,
    };
    let seeds: Vec<PlayableLink> = hit
        .title
        .playing_order()
        .into_iter()
        .filter(|cached| cached.health != title_cache::LinkHealth::Broken)
        .take(take)
        .map(|cached| cached.link.clone())
        .collect();
    if seeds.is_empty() {
        return;
    }
    tracing::info!(
        ?hit.freshness,
        seeds = seeds.len(),
        "planting saved sources for open"
    );
    for link in seeds {
        state.ordered.push((link, None));
    }
    (hooks.on_progress)(state.progress());
    (hooks.on_update)(&state.snapshot());
    match hit.freshness {
        Freshness::Fresh => {
            if let Some((link, slot)) = state.ordered.first_mut() {
                let health = checker.check(link);
                let ok = health.is_working();
                *slot = Some(health);
                state.checked = 1;
                state.next_to_check = 1;
                if ok {
                    state.working = 1;
                    state.play_index = Some(0);
                    state.trusted_played = true;
                    state.announce_ready(hooks);
                }
            }
            check_pending(state, checker, hooks);
        }
        Freshness::Stale => {
            let deadline = Instant::now() + Duration::from_secs(STALE_TEST_SECS);
            while state.next_to_check < state.ordered.len() && Instant::now() < deadline {
                if state.play_index.is_some() {
                    break;
                }
                check_one(state, checker, hooks);
            }
        }
    }
}

pub(crate) fn run_search(
    mut search: LinkSearch,
    mut state: OpenState,
    checker: &StreamChecker,
    hooks: &OpenHooks<'_>,
) -> Result<CheckedOpen, OpenError> {
    loop {
        if hooks.cancel.load(Ordering::SeqCst) {
            break;
        }
        if state.timed_out() {
            break;
        }
        if hooks.skip.load(Ordering::SeqCst) && state.play_index.is_some() {
            break;
        }
        if state.first_working_wait_done() && hooks.skip.load(Ordering::SeqCst) {
            break;
        }
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
        if batch.done {
            state.search_done = true;
            (hooks.on_progress)(state.progress());
            while state.next_to_check < state.ordered.len() {
                if state.timed_out() || hooks.cancel.load(Ordering::SeqCst) {
                    break;
                }
                if hooks.skip.load(Ordering::SeqCst) && state.play_index.is_some() {
                    break;
                }
                check_one(&mut state, checker, hooks);
            }
            break;
        }
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

fn check_pending(state: &mut OpenState, checker: &StreamChecker, hooks: &OpenHooks<'_>) {
    while state.next_to_check < state.ordered.len() {
        if state.timed_out() || hooks.cancel.load(Ordering::SeqCst) {
            break;
        }
        if hooks.skip.load(Ordering::SeqCst) && state.play_index.is_some() {
            break;
        }
        check_one(state, checker, hooks);
    }
}

fn check_one(state: &mut OpenState, checker: &StreamChecker, hooks: &OpenHooks<'_>) {
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
        if state.play_index.is_none() {
            state.play_index = Some(index);
        }
    }
    state.next_to_check += 1;
    (hooks.on_progress)(state.progress());
    (hooks.on_update)(&state.snapshot());
    state.announce_ready(hooks);
}

pub(crate) fn finish(state: OpenState) -> Result<CheckedOpen, OpenError> {
    if state.ordered.is_empty() {
        return Err(OpenError::Empty);
    }
    Ok(state.snapshot())
}

// Silence unused until open_with reads it.
#[allow(dead_code)]
fn _trusted(state: &OpenState) -> bool {
    state.trusted_played
}
