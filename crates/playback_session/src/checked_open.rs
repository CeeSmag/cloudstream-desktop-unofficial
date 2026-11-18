//! Open a title by searching links and HTTP-checking them as they arrive.

use std::sync::Arc;
use std::time::{Duration, Instant};

use app_services::{LinkHealth, LinkSearch, StreamChecker, StreamResolver};
use core_model::{PlayableLink, SubtitleTrack};
use extension_repo::UreqProbe;
use thiserror::Error;

/// Cap how long we wait for the first working stream before returning what we have.
const FIRST_WORKING_SECS: u64 = 30;
/// Cap for the whole search + check pass.
const TOTAL_SECS: u64 = 60;
/// How many extra working candidates to keep checking after the first success.
const EXTRA_AFTER_FIRST: usize = 2;

/// Outcome of a checked open: streams ordered best-first with health, plus which to play.
#[derive(Debug, Clone)]
pub struct CheckedOpen {
    /// Links in play order with their check result (`None` = still unchecked when we stopped).
    pub links: Vec<(PlayableLink, Option<LinkHealth>)>,
    /// Index into `links` of the stream to start, when one works.
    pub play_index: Option<usize>,
    /// Subtitles gathered from the provider search.
    pub subtitles: Vec<SubtitleTrack>,
}

/// Progress while searching and checking (for UI status lines).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenProgress {
    /// Streams found so far.
    pub found: usize,
    /// Streams already checked.
    pub checked: usize,
    /// Streams that answered as working.
    pub working: usize,
}

/// Why a checked open failed before any playable stream was ready.
#[derive(Debug, Error)]
pub enum OpenError {
    /// Provider / gateway failure while starting or reading the search.
    #[error("{0}")]
    Search(String),
    /// Search finished with nothing the player can open.
    #[error("no playable streams found")]
    Empty,
}

/// Searches for streams of `provider`/`data`, checks them, and picks the best working one.
///
/// `on_progress` is called as counts change.
pub fn check_and_pick(
    streams: &StreamResolver,
    provider: &str,
    data: &str,
    on_progress: &(dyn Fn(OpenProgress) + Sync),
) -> Result<CheckedOpen, OpenError> {
    let search = streams
        .open_search(provider, data)
        .map_err(|error| OpenError::Search(error.to_string()))?;
    run_search(search, on_progress)
}

struct OpenState {
    ordered: Vec<(PlayableLink, Option<LinkHealth>)>,
    subtitles: Vec<SubtitleTrack>,
    checked: usize,
    working: usize,
    play_index: Option<usize>,
    next_to_check: usize,
    extras_after_first: usize,
    started: Instant,
}

impl OpenState {
    fn new() -> Self {
        Self {
            ordered: Vec::new(),
            subtitles: Vec::new(),
            checked: 0,
            working: 0,
            play_index: None,
            next_to_check: 0,
            extras_after_first: 0,
            started: Instant::now(),
        }
    }

    fn progress(&self) -> OpenProgress {
        OpenProgress {
            found: self.ordered.len(),
            checked: self.checked,
            working: self.working,
        }
    }

    fn timed_out(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(TOTAL_SECS)
    }

    fn first_working_cap(&self) -> bool {
        self.play_index.is_some()
            && self.started.elapsed() >= Duration::from_secs(FIRST_WORKING_SECS)
    }

    fn enough_extras(&self) -> bool {
        self.play_index.is_some() && self.extras_after_first >= EXTRA_AFTER_FIRST
    }
}

fn run_search(
    mut search: LinkSearch,
    on_progress: &(dyn Fn(OpenProgress) + Sync),
) -> Result<CheckedOpen, OpenError> {
    let checker = StreamChecker::new(Arc::new(UreqProbe::default()));
    let mut state = OpenState::new();
    loop {
        if state.timed_out() || state.first_working_cap() {
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
        absorb_batch(&mut state, batch.links, batch.subtitles, on_progress);
        check_pending(&mut state, &checker, on_progress);
        if batch.done || state.enough_extras() {
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
    on_progress: &(dyn Fn(OpenProgress) + Sync),
) {
    for link in links {
        state.ordered.push((link, None));
    }
    for track in tracks {
        if !state.subtitles.iter().any(|known| known.url == track.url) {
            state.subtitles.push(track);
        }
    }
    on_progress(state.progress());
}

fn check_pending(
    state: &mut OpenState,
    checker: &StreamChecker,
    on_progress: &(dyn Fn(OpenProgress) + Sync),
) {
    while state.next_to_check < state.ordered.len() {
        if state.timed_out() || state.enough_extras() {
            break;
        }
        let index = state.next_to_check;
        let Some((link, slot)) = state.ordered.get_mut(index) else {
            break;
        };
        let health = checker.check(link);
        let is_ok = health.is_working();
        *slot = Some(health);
        state.checked += 1;
        if is_ok {
            state.working += 1;
            if state.play_index.is_none() {
                state.play_index = Some(index);
            } else {
                state.extras_after_first += 1;
            }
        }
        state.next_to_check += 1;
        on_progress(state.progress());
        if state.first_working_cap() {
            break;
        }
    }
}

fn finish(state: OpenState) -> Result<CheckedOpen, OpenError> {
    if state.ordered.is_empty() {
        return Err(OpenError::Empty);
    }
    Ok(CheckedOpen {
        links: state.ordered,
        play_index: state.play_index,
        subtitles: state.subtitles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_progress_defaults_are_zero() {
        let progress = OpenProgress {
            found: 0,
            checked: 0,
            working: 0,
        };
        assert_eq!(progress.found, 0);
    }
}
