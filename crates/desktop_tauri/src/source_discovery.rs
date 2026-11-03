//! The work behind opening a video: one thread follows the provider's search and hands every new
//! stream to the checkers, the other thread checks the streams as they come.

use std::sync::Arc;
use std::time::{Duration, Instant};

use app_services::{link_rank, CheckQueue, LinkBatch, LinkHealth, LinkSearch, StreamChecker};
use core_model::SubtitleTrack;

use crate::check_gate::CheckGate;
use crate::source_manager::CheckUpdate;
use crate::source_session::{SourceHealth, SourceSession};

/// How long one look at the search waits for news before the video is asked about again.
const POLL_WAIT: Duration = Duration::from_millis(500);
/// Pause after a look that found something, so streams that arrive together are handled together
/// and the runtime's single channel is left free for other calls.
const BATCH_PAUSE: Duration = Duration::from_millis(150);
/// The picker is refreshed after this many new results.
const NOTIFY_EVERY: usize = 8;

/// Hands subtitles that were found after playback started to the player.
pub type SubtitleSink = Arc<dyn Fn(&[SubtitleTrack]) + Send + Sync>;

/// Everything the search and check threads need for one video.
pub struct Discovery {
    pub(crate) checker: StreamChecker,
    pub(crate) session: Arc<SourceSession>,
    pub(crate) gate: Arc<CheckGate>,
    pub(crate) generation: u64,
    pub(crate) queue: Arc<CheckQueue>,
    pub(crate) started: Instant,
    pub(crate) tell: Arc<dyn Fn(CheckUpdate) + Send + Sync>,
    pub(crate) give_subtitles: SubtitleSink,
}

impl Discovery {
    fn wanted(&self) -> bool {
        self.session.is_current(self.generation) && !self.gate.is_cancelled()
    }

    /// Follows `search` until it ends or the video is closed.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn run_search(&self, mut search: LinkSearch) {
        loop {
            if !self.wanted() {
                search.cancel();
                self.queue.stop();
                tracing::info!(
                    elapsed_ms = self.started.elapsed().as_millis(),
                    "stream search stopped: the video was closed"
                );
                return;
            }
            let batch = match search.poll(POLL_WAIT) {
                Ok(batch) => batch,
                Err(error) => {
                    tracing::warn!(%error, "the stream search failed");
                    self.end_search(Some(error.to_string()));
                    return;
                }
            };
            let (done, failure) = (batch.done, batch.failure.clone());
            let news = self.take(batch);
            if done {
                self.end_search(failure);
                return;
            }
            if news {
                std::thread::sleep(BATCH_PAUSE);
            }
        }
    }

    /// Checks the streams as they are found, until the search is over and all are answered.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn run_checks(&self) {
        self.checker
            .check_queue(&self.queue, &|| self.wanted(), &|index, found| {
                self.record(index, &found)
            });
        let progress = self.gate.progress();
        if !self.wanted() {
            tracing::info!(
                answered = progress.checked,
                of = progress.total,
                elapsed_ms = self.started.elapsed().as_millis(),
                "stream checks cancelled: the video was closed"
            );
            return;
        }
        (self.tell)(CheckUpdate::SourcesChanged);
        tracing::info!(
            checked = progress.checked,
            working = progress.working,
            elapsed_ms = self.started.elapsed().as_millis(),
            "stream checks finished"
        );
    }

    /// Remembers what the search found and queues the streams for checking.
    /// Returns whether there was anything new.
    fn take(&self, batch: LinkBatch) -> bool {
        if batch.links.is_empty() && batch.subtitles.is_empty() {
            return false;
        }
        let ranks: Vec<i32> = batch.links.iter().map(link_rank).collect();
        let found = batch.links.len();
        let queued = batch.links.clone();
        let Some(first) = self
            .session
            .append(self.generation, batch.links, batch.subtitles)
        else {
            return false;
        };
        let progress = self.gate.announce(&ranks);
        self.queue.push(
            queued
                .into_iter()
                .enumerate()
                .map(|(offset, link)| (first + offset, link)),
        );
        tracing::info!(
            found,
            total = progress.total,
            elapsed_ms = self.started.elapsed().as_millis(),
            "streams found"
        );
        (self.tell)(CheckUpdate::Progress(progress));
        (self.tell)(CheckUpdate::SourcesChanged);
        self.pass_on_subtitles();
        true
    }

    fn pass_on_subtitles(&self) {
        let fresh = self.session.unsent_subtitles(self.generation);
        if !fresh.is_empty() {
            (self.give_subtitles)(&fresh);
        }
    }

    fn end_search(&self, failure: Option<String>) {
        self.queue.close();
        let progress = self.gate.discovery_ended(failure);
        (self.tell)(CheckUpdate::Progress(progress));
        tracing::info!(
            total = progress.total,
            elapsed_ms = self.started.elapsed().as_millis(),
            "stream search finished"
        );
    }

    /// Stores one answer; `false` tells the checker nobody wants more answers.
    fn record(&self, index: usize, found: &LinkHealth) -> bool {
        if let LinkHealth::Broken(reason) = found {
            tracing::debug!(index, %reason, "stream is not playable");
        }
        let progress = self.gate.record(index, found.is_working());
        (self.tell)(CheckUpdate::Progress(progress));
        let health = if found.is_working() {
            SourceHealth::Working
        } else {
            SourceHealth::Broken
        };
        if !self.session.set_health(self.generation, index, health) {
            return false;
        }
        if let LinkHealth::Working {
            size_bytes: Some(size),
        } = found
        {
            self.session.set_size(self.generation, index, *size);
        }
        let first_working = found.is_working() && progress.working == 1;
        if first_working || progress.checked.is_multiple_of(NOTIFY_EVERY) {
            (self.tell)(CheckUpdate::SourcesChanged);
        }
        true
    }
}

#[cfg(test)]
#[path = "source_discovery_tests.rs"]
mod tests;
