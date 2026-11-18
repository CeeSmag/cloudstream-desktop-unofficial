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

fn run_search(
    mut search: LinkSearch,
    on_progress: &(dyn Fn(OpenProgress) + Sync),
) -> Result<CheckedOpen, OpenError> {
    let checker = StreamChecker::new(Arc::new(UreqProbe::default()));
    let started = Instant::now();
    let mut ordered: Vec<(PlayableLink, Option<LinkHealth>)> = Vec::new();
    let mut subtitles: Vec<SubtitleTrack> = Vec::new();
    let mut checked = 0usize;
    let mut working = 0usize;
    let mut play_index = None;
    let mut next_to_check = 0usize;
    let mut extras_after_first = 0usize;

    loop {
        if started.elapsed() >= Duration::from_secs(TOTAL_SECS) {
            break;
        }
        if play_index.is_some() && started.elapsed() >= Duration::from_secs(FIRST_WORKING_SECS) {
            break;
        }

        let batch = search
            .poll(Duration::from_millis(200))
            .map_err(|error| OpenError::Search(error.to_string()))?;
        if let Some(failure) = &batch.failure {
            if ordered.is_empty() {
                search.cancel();
                return Err(OpenError::Search(failure.clone()));
            }
        }
        for link in batch.links {
            ordered.push((link, None));
        }
        for track in batch.subtitles {
            if !subtitles.iter().any(|known| known.url == track.url) {
                subtitles.push(track);
            }
        }
        on_progress(OpenProgress {
            found: ordered.len(),
            checked,
            working,
        });

        while next_to_check < ordered.len() {
            if started.elapsed() >= Duration::from_secs(TOTAL_SECS) {
                break;
            }
            if play_index.is_some() && extras_after_first >= EXTRA_AFTER_FIRST {
                break;
            }
            let Some((link, slot)) = ordered.get_mut(next_to_check) else {
                break;
            };
            let health = checker.check(link);
            let is_ok = health.is_working();
            *slot = Some(health);
            checked += 1;
            if is_ok {
                working += 1;
                if play_index.is_none() {
                    play_index = Some(next_to_check);
                } else {
                    extras_after_first += 1;
                }
            }
            next_to_check += 1;
            on_progress(OpenProgress {
                found: ordered.len(),
                checked,
                working,
            });
            if play_index.is_some() && started.elapsed() >= Duration::from_secs(FIRST_WORKING_SECS)
            {
                break;
            }
        }

        if batch.done {
            break;
        }
        if play_index.is_some() && extras_after_first >= EXTRA_AFTER_FIRST {
            break;
        }
    }

    search.cancel();
    if ordered.is_empty() {
        return Err(OpenError::Empty);
    }
    Ok(CheckedOpen {
        links: ordered,
        play_index,
        subtitles,
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
