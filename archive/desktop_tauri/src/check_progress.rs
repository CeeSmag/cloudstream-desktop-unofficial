//! The values the gate hands to the rest of the app: timings, progress and the start decision.

use std::time::Duration;

use serde::Serialize;

/// The longest a video waits for its provider; both are counted from when it was opened.
#[derive(Debug, Clone, Copy)]
pub struct WaitLimits {
    /// The longest the provider may take to give a first stream; later it is an error.
    pub first_stream: Duration,
    /// The longest the whole search and all the checks may take before playback starts anyway on
    /// the best stream known, or fails when there is none.
    pub max_wait: Duration,
}

impl Default for WaitLimits {
    fn default() -> Self {
        Self {
            first_stream: Duration::from_secs(30),
            max_wait: Duration::from_secs(60),
        }
    }
}

/// What the loading screen says about the sources saved from an earlier time.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SavedSources {
    /// Nothing was saved for this video, or the saved sources are not in use.
    #[default]
    None,
    /// Saved a short while ago: playing from them at once.
    Trusted,
    /// Saved a while ago: the best of them are being tested before playing.
    Testing,
    /// None of the tested ones works any more: searching again.
    OutOfDate,
}

/// How far the search and the checks have come; sent to the UI.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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
    /// What is going on with sources saved earlier.
    pub saved: SavedSources,
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
