//! The work behind opening a video: one thread follows the provider's search and hands every new
//! stream to the checkers, the other thread checks the streams as they come.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use app_services::{
    link_rank, stream_host, CheckQueue, LinkBatch, LinkHealth, LinkSearch, StreamChecker,
};
use core_model::SubtitleTrack;
use subtitle_store::SubtitlePrefetch;

use crate::check_gate::CheckGate;
use crate::failure_tally::FailureTally;
use crate::source_manager::CheckUpdate;
use crate::source_session::{SourceHealth, SourceSession};

/// How long one look at the search waits for news before the video is asked about again.
const POLL_WAIT: Duration = Duration::from_millis(250);
/// Pause after a look that found something, so streams that arrive together are handled together.
const BATCH_PAUSE: Duration = Duration::from_millis(150);
/// Pause after a look that found nothing; the runtime has one channel for all calls and this
/// leaves it free so browsing and the next video are not starved by the long poll.
const IDLE_PAUSE: Duration = Duration::from_millis(50);
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
    /// Downloads the subtitle files the search names.
    pub(crate) prefetch: SubtitlePrefetch,
    /// The host of every stream found, by index, for the failure log.
    pub(crate) hosts: Mutex<Vec<String>>,
    /// Why the checks of this video failed.
    pub(crate) failures: FailureTally,
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
                self.prefetch.stop();
                tracing::info!(
                    elapsed_ms = self.started.elapsed().as_millis(),
                    "stream search stopped: the video was closed"
                );
                return;
            }
            let batch = match search.poll(POLL_WAIT) {
                Ok(batch) => batch,
                Err(error) => {
                    if self.wanted() {
                        tracing::warn!(%error, "the stream search failed");
                    } else {
                        tracing::debug!(%error, "the stream search ended after the video was closed");
                    }
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
            std::thread::sleep(if news { BATCH_PAUSE } else { IDLE_PAUSE });
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
                failed = self.failures.total(),
                elapsed_ms = self.started.elapsed().as_millis(),
                "stream checks cancelled: the video was closed"
            );
            return;
        }
        (self.tell)(CheckUpdate::SourcesChanged);
        tracing::info!(
            checked = progress.checked,
            working = progress.working,
            failed = self.failures.total(),
            failure_reasons = %self.failures.reasons(),
            failed_hosts = %self.failures.hosts(),
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
        let subtitles = batch.subtitles.len();
        self.prefetch.submit(batch.subtitles);
        if batch.links.is_empty() {
            tracing::info!(subtitles, "subtitle files found; downloading them");
            return true;
        }
        let ranks: Vec<i32> = batch.links.iter().map(link_rank).collect();
        let found = batch.links.len();
        let queued = batch.links.clone();
        let hosts: Vec<String> = queued.iter().map(stream_host).collect();
        let Some(first) = self
            .session
            .append(self.generation, batch.links, Vec::new())
        else {
            return false;
        };
        self.hosts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(hosts);
        let progress = self.gate.announce(&ranks);
        self.queue.push(
            queued
                .into_iter()
                .enumerate()
                .map(|(offset, link)| (first + offset, link)),
        );
        tracing::info!(
            found,
            subtitles,
            total = progress.total,
            elapsed_ms = self.started.elapsed().as_millis(),
            "streams found"
        );
        (self.tell)(CheckUpdate::Progress(progress));
        (self.tell)(CheckUpdate::SourcesChanged);
        true
    }

    fn end_search(&self, failure: Option<String>) {
        self.queue.close();
        self.prefetch.finish();
        let progress = self.gate.discovery_ended(failure);
        (self.tell)(CheckUpdate::Progress(progress));
        tracing::info!(
            total = progress.total,
            elapsed_ms = self.started.elapsed().as_millis(),
            "stream search finished"
        );
    }

    fn host_of(&self, index: usize) -> String {
        self.hosts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(index)
            .cloned()
            .unwrap_or_default()
    }

    /// Stores one answer; `false` tells the checker nobody wants more answers.
    fn record(&self, index: usize, found: &LinkHealth) -> bool {
        if let LinkHealth::Broken(reason) = found {
            let host = self.host_of(index);
            tracing::debug!(index, %host, %reason, "stream is not playable");
            self.failures.record(&host, reason);
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
            size_bytes,
            subtitles,
        } = found
        {
            if let Some(size) = size_bytes {
                self.session.set_size(self.generation, index, *size);
            }
            if !subtitles.is_empty() {
                self.session
                    .set_stream_subtitles(self.generation, index, subtitles.clone());
            }
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
