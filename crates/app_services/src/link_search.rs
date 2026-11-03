use std::sync::Arc;
use std::time::Duration;

use core_model::{LinkSearchCursor, PlayableLink, SubtitleTrack};
use extension_api::{GatewayError, ProviderGateway};

use crate::stream_resolver::playable_links_best_first;
use crate::{ServiceError, StreamResolver};

/// How many searches are started when one ends without a single playable stream.
const MAX_ATTEMPTS: u32 = 3;

/// What a running search found since it was last asked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkBatch {
    /// New streams a player can open (best first within the batch).
    pub links: Vec<PlayableLink>,
    /// New external subtitles.
    pub subtitles: Vec<SubtitleTrack>,
    /// True when the search is over and nothing more will come.
    pub done: bool,
    /// Why the search failed, when it ended without any stream because the provider failed.
    pub failure: Option<String>,
}

/// A search for the streams of one title that reports them while it still runs, so playback can
/// start on the first good one instead of waiting for every host to answer.
///
/// Extensions swallow network errors and report "success" with no links, so a search that ends
/// empty is started again; after the last attempt a URL payload is tried on the extractor registry.
pub struct LinkSearch {
    gateway: Arc<dyn ProviderGateway>,
    provider: String,
    data: String,
    /// Id of the running search; `None` when the runtime can only answer in one piece.
    id: Option<String>,
    cursor: LinkSearchCursor,
    attempt: u32,
    /// Playable streams found over all attempts.
    found: usize,
}

impl LinkSearch {
    /// Starts looking for the streams of `data` (a movie payload or an episode payload).
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider), err)]
    pub fn start(
        gateway: Arc<dyn ProviderGateway>,
        provider: &str,
        data: &str,
    ) -> Result<Self, ServiceError> {
        let mut search = Self {
            gateway,
            provider: provider.to_owned(),
            data: data.to_owned(),
            id: None,
            cursor: LinkSearchCursor::default(),
            attempt: 1,
            found: 0,
        };
        tracing::info!(provider, "finding streams");
        search.begin_run()?;
        Ok(search)
    }

    /// What was found since the last call; waits up to `wait` for news when there is none yet.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn poll(&mut self, wait: Duration) -> Result<LinkBatch, ServiceError> {
        let Some(id) = self.id.clone() else {
            return self.resolve_in_one_piece();
        };
        let update = self.gateway.poll_link_search(&id, self.cursor, wait)?;
        self.cursor = update.cursor();
        let mut batch = LinkBatch {
            links: playable_links_best_first(update.links),
            subtitles: update.subtitles,
            ..LinkBatch::default()
        };
        self.found += batch.links.len();
        if update.done {
            self.end_of_run(update.failure, &mut batch)?;
        }
        Ok(batch)
    }

    /// Stops the search because nobody wants its streams any more.
    pub fn cancel(&self) {
        let Some(id) = &self.id else { return };
        if let Err(error) = self.gateway.cancel_link_search(id) {
            tracing::warn!(provider = %self.provider, %error, "could not stop the link search");
        }
    }

    fn begin_run(&mut self) -> Result<(), ServiceError> {
        match self.gateway.start_link_search(&self.provider, &self.data) {
            Ok(id) => {
                self.id = Some(id);
                self.cursor = LinkSearchCursor::default();
                Ok(())
            }
            Err(GatewayError::Unavailable(reason)) => {
                tracing::info!(provider = %self.provider, %reason, "streams will arrive in one piece");
                self.id = None;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// A run ended: finish, or start another run when it found nothing.
    fn end_of_run(
        &mut self,
        failure: Option<String>,
        batch: &mut LinkBatch,
    ) -> Result<(), ServiceError> {
        if self.found > 0 || failure.is_some() {
            batch.done = true;
            batch.failure = failure.filter(|_| self.found == 0);
            return Ok(());
        }
        if self.attempt < MAX_ATTEMPTS {
            self.attempt += 1;
            tracing::warn!(provider = %self.provider, attempt = self.attempt, "provider returned no playable streams; searching again");
            return self.begin_run();
        }
        batch.done = true;
        if self.data.starts_with("http") {
            tracing::info!(data = %self.data, "falling back to the extractor registry");
            let resolved = self.gateway.extract(&self.data, None)?;
            batch.links = playable_links_best_first(resolved.links);
            batch.subtitles.extend(resolved.subtitles);
        }
        Ok(())
    }

    /// For runtimes without streaming: the whole answer arrives at once.
    fn resolve_in_one_piece(&mut self) -> Result<LinkBatch, ServiceError> {
        let resolved =
            StreamResolver::new(Arc::clone(&self.gateway)).resolve(&self.provider, &self.data)?;
        Ok(LinkBatch {
            links: playable_links_best_first(resolved.links),
            subtitles: resolved.subtitles,
            done: true,
            failure: None,
        })
    }
}

#[cfg(test)]
#[path = "link_search_tests.rs"]
mod tests;
