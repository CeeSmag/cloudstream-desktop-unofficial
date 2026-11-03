//! When the stream that plays fails, switches to the next best source by itself.
//!
//! The player reports a failed stream; the controller marks it broken (in the list and in the
//! saved sources), picks the best source not known to be broken (working ones first, then ones
//! still being checked) and starts it at the position where the video stopped. It gives up after
//! [`FallbackLimits::attempts`] sources or [`FallbackLimits::total`] time, whichever comes first,
//! with one message that names every reason. A stream that plays well, a manual switch or a new
//! video starts the count over.

use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use app_services::PlaybackService;
use player_api::{PlaybackState, PlaybackStatus};
use title_cache::redact_url;

use crate::source_fallback_steps::{
    next_candidate, none_left_message, sources_word, start_source, switched,
};
use crate::source_manager::SourceManager;
use crate::source_session::{ActiveSources, SourceHealth};
use crate::video_opening::title_key;

/// How far the fallback goes before it gives up.
#[derive(Debug, Clone, Copy)]
pub struct FallbackLimits {
    /// The most sources tried, the one that failed first included.
    pub attempts: usize,
    /// The longest the whole fallback may take, counted from the first failure.
    pub total: Duration,
}

impl Default for FallbackLimits {
    fn default() -> Self {
        Self {
            attempts: 3,
            total: Duration::from_secs(20),
        }
    }
}

/// What a look at the player's state led to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackOutcome {
    /// Nothing is wrong, or nothing is open.
    Fine,
    /// Another call is switching right now.
    Switching,
    /// The source with this index was started instead of the failed one.
    Switched(usize),
    /// No source is left to try; the text is for the viewer.
    GaveUp(String),
}

/// What the fallback works with.
#[derive(Clone, Copy)]
pub(crate) struct Services<'a> {
    pub(crate) sources: &'a SourceManager,
    pub(crate) playback: &'a PlaybackService,
}

/// The failures of one video since a stream last played well.
struct Run {
    generation: u64,
    started: Instant,
    reasons: Vec<String>,
    gave_up: Option<String>,
}

/// Watches the player's state and switches away from failed sources.
#[derive(Default)]
pub struct FallbackController {
    limits: FallbackLimits,
    run: Mutex<Option<Run>>,
    /// Held while a source is being switched, so one failure is handled by one caller.
    busy: Mutex<()>,
}

impl FallbackController {
    /// A controller with its own limits.
    #[cfg(test)]
    pub fn new(limits: FallbackLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    fn lock_run(&self) -> MutexGuard<'_, Option<Run>> {
        self.run.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts the count over: a manual switch, or a stream that plays well.
    pub fn reset(&self) {
        self.lock_run().take();
    }

    /// Looks at the player's state. A failed stream is replaced by the next source; a stream that
    /// plays ends the run of failures and is remembered as the one that worked.
    pub fn on_status(
        &self,
        sources: &SourceManager,
        playback: &PlaybackService,
        status: &PlaybackStatus,
    ) -> FallbackOutcome {
        match &status.state {
            PlaybackState::Failed(reason) => {
                self.on_failure(Services { sources, playback }, reason)
            }
            PlaybackState::Playing | PlaybackState::Paused => {
                self.on_playing(sources);
                FallbackOutcome::Fine
            }
            _ => FallbackOutcome::Fine,
        }
    }

    fn on_playing(&self, sources: &SourceManager) {
        let had_failures = self
            .lock_run()
            .take()
            .is_some_and(|run| !run.reasons.is_empty());
        if !had_failures {
            return;
        }
        if let Some(active) = sources.session().snapshot() {
            if let Some(link) = active.links.get(active.selected) {
                tracing::info!(stream = %redact_url(&link.url), "the fallback source plays");
                sources.note_played(&title_key(&active.item), &link.url);
            }
        }
    }

    fn on_failure(&self, services: Services<'_>, reason: &str) -> FallbackOutcome {
        let _busy = match self.busy.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return FallbackOutcome::Switching,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        let Some(generation) = services
            .sources
            .session()
            .snapshot()
            .map(|active| active.generation)
        else {
            return FallbackOutcome::Fine;
        };
        let mut run = self.take_run(generation);
        if let Some(message) = run.gave_up.clone() {
            *self.lock_run() = Some(run);
            return FallbackOutcome::GaveUp(message);
        }
        let outcome = self.fail_over(services, &mut run, reason);
        *self.lock_run() = Some(run);
        outcome
    }

    /// The run of this video, or a new one when there is none or it belongs to another video.
    fn take_run(&self, generation: u64) -> Run {
        self.lock_run()
            .take()
            .filter(|run| run.generation == generation)
            .unwrap_or_else(|| Run {
                generation,
                started: Instant::now(),
                reasons: Vec::new(),
                gave_up: None,
            })
    }

    /// Marks the failed stream, then starts the next candidate until one starts or the limits end.
    fn fail_over(&self, services: Services<'_>, run: &mut Run, reason: &str) -> FallbackOutcome {
        let Services { sources, playback } = services;
        let mut reason = reason.to_owned();
        let mut failed = None;
        loop {
            let Some(active) = sources.session().start_snapshot() else {
                return FallbackOutcome::Fine;
            };
            let failed_index = failed.unwrap_or(active.selected);
            record_failure(sources, (&active, failed_index), run, &reason);
            if let Some(message) = self.limit_message(run) {
                return give_up(run, message);
            }
            let Some(next) = next_candidate(&active, failed_index) else {
                return give_up(run, none_left_message(&run.reasons));
            };
            match start_source(playback, &active, next) {
                Ok(()) => return switched(services, (&active, next)),
                Err(error) => {
                    failed = Some(next);
                    reason = error;
                }
            }
        }
    }

    fn limit_message(&self, run: &Run) -> Option<String> {
        let tried = run.reasons.len();
        if tried >= self.limits.attempts {
            return Some(format!(
                "{tried} sources failed ({}). Pick another source from the list.",
                run.reasons.join("; ")
            ));
        }
        (run.started.elapsed() >= self.limits.total).then(|| {
            format!(
                "{} failed ({}) and trying more took too long. Pick another source from the list.",
                sources_word(tried),
                run.reasons.join("; ")
            )
        })
    }
}

/// Marks the failed stream broken in the list and in the saved sources, and counts the failure.
fn record_failure(
    sources: &SourceManager,
    failed: (&ActiveSources, usize),
    run: &mut Run,
    reason: &str,
) {
    let (active, index) = failed;
    sources
        .session()
        .set_health(active.generation, index, SourceHealth::Broken);
    let stream = active.links.get(index).map(|link| link.url.as_str());
    tracing::warn!(
        attempt = run.reasons.len() + 1,
        stream = %stream.map(redact_url).unwrap_or_default(),
        %reason,
        elapsed_ms = run.started.elapsed().as_millis(),
        "a stream failed"
    );
    if let Some(url) = stream {
        sources.note_broken(&title_key(&active.item), url);
    }
    run.reasons.push(reason.to_owned());
}

fn give_up(run: &mut Run, message: String) -> FallbackOutcome {
    tracing::error!(tried = run.reasons.len(), %message, "no source could be played");
    run.gave_up = Some(message.clone());
    FallbackOutcome::GaveUp(message)
}

#[cfg(test)]
#[path = "source_fallback_tests.rs"]
mod tests;
