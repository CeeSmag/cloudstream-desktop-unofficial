//! The values the gate hands to the rest of the app: timings, progress and the start decision.

use std::time::Duration;

use serde::Serialize;

/// How long the video waits for streams.
#[derive(Debug, Clone, Copy)]
pub struct Grace {
    /// Wait for a better stream after the first working one, when the best so far is high quality.
    pub high_quality: Duration,
    /// The same wait otherwise.
    pub other: Duration,
    /// The longest the video may wait for anything, counted from when it was opened.
    pub max_wait: Duration,
}

impl Default for Grace {
    fn default() -> Self {
        Self {
            high_quality: Duration::from_millis(900),
            other: Duration::from_millis(2000),
            max_wait: Duration::from_secs(5),
        }
    }
}

/// How far the search and the checks have come; sent to the UI.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct CheckProgress {
    /// Streams answered so far.
    pub checked: usize,
    /// Streams found so far (it grows while the provider is still searching).
    pub total: usize,
    /// Of the answered ones, how many can be played.
    pub working: usize,
    /// The viewer asked to start without waiting for the rest.
    pub skipping: bool,
    /// The provider is still looking for streams.
    pub discovering: bool,
}

/// What the wait for playback came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartDecision {
    /// Play the stream with this index.
    Start(usize),
    /// The video was closed meanwhile.
    Cancelled,
    /// There is nothing to play; the text says why.
    Failed(String),
}
