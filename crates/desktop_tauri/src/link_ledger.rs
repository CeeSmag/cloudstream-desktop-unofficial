//! Keeps count of the streams of the video that is being opened, in the same order as the open
//! session, so the saved sources can be written when the search and the checks are done or the
//! video is closed. It owns its own copy, since the session is cleared when the video closes.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use core_model::PlayableLink;
use title_cache::{CachedLink, CachedTitle, LinkHealth, TitleCache};

/// How the provider's search ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchEnd {
    Running,
    Finished,
    Failed,
}

struct LedgerState {
    /// Every stream of the session, by its index there.
    links: Vec<CachedLink>,
    /// The addresses in `links`.
    known: HashSet<String>,
    /// What was saved earlier and is not in the session; kept when this search is cut short.
    rest: CachedTitle,
    search: SearchEnd,
}

impl LedgerState {
    /// What to save: the session's streams, and when the search did not finish, whatever else was
    /// saved earlier (the fresh search is authoritative only when it finished).
    fn title(&self, complete: bool, last_good: Option<String>) -> CachedTitle {
        let mut links = self.links.clone();
        if !complete {
            links.extend(
                self.rest
                    .links
                    .iter()
                    .filter(|saved| !self.known.contains(&saved.link.url))
                    .cloned(),
            );
        }
        CachedTitle {
            complete,
            links,
            last_good: last_good.or_else(|| self.rest.last_good.clone()),
        }
    }

    fn working(&self) -> usize {
        self.links
            .iter()
            .filter(|cached| cached.health == LinkHealth::Working)
            .count()
    }
}

/// The streams of one video and the saved sources of its title.
pub struct LinkLedger {
    cache: Arc<TitleCache>,
    key: String,
    state: Mutex<LedgerState>,
}

impl LinkLedger {
    /// A ledger for the title `key`; `rest` is what was saved earlier and is not planted in the
    /// session.
    pub fn new(cache: Arc<TitleCache>, key: String, rest: CachedTitle) -> Self {
        Self {
            cache,
            key,
            state: Mutex::new(LedgerState {
                links: Vec::new(),
                known: HashSet::new(),
                rest,
                search: SearchEnd::Running,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, LedgerState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds streams that were saved earlier, in the order they are put in the session.
    pub fn plant(&self, saved: &[CachedLink]) {
        let mut state = self.lock();
        for cached in saved {
            if state.known.insert(cached.link.url.clone()) {
                state.links.push(cached.clone());
            }
        }
    }

    /// Adds streams the provider found and returns only those not seen before, which are the ones
    /// to put in the session (so the session and the ledger keep the same order).
    pub fn fresh(&self, found: Vec<PlayableLink>) -> Vec<PlayableLink> {
        let mut state = self.lock();
        let mut added = Vec::new();
        for link in found {
            if state.known.insert(link.url.clone()) {
                state.links.push(CachedLink::unchecked(link.clone()));
                added.push(link);
            }
        }
        added
    }

    /// The check of the stream at `index` came back.
    pub fn answered(&self, index: usize, working: bool) {
        let mut state = self.lock();
        if let Some(cached) = state.links.get_mut(index) {
            cached.health = if working {
                LinkHealth::Working
            } else {
                LinkHealth::Broken
            };
        }
    }

    /// The provider's search is over; `completed` is false when it failed.
    pub fn search_ended(&self, completed: bool) {
        self.lock().search = if completed {
            SearchEnd::Finished
        } else {
            SearchEnd::Failed
        };
    }

    /// Writes the saved sources. `closed` says the video was closed before everything was done, so
    /// what is known is saved as partial. A finished search that found nothing working removes the
    /// saved sources, since they are dead. Problems are logged and never reach the viewer.
    pub fn save(&self, last_good: Option<String>, closed: bool) {
        let state = self.lock();
        let complete = !closed && state.search == SearchEnd::Finished;
        let working = state.working();
        if working == 0 {
            drop(state);
            self.forget_if_dead(complete);
            return;
        }
        let title = state.title(complete, last_good);
        drop(state);
        match self.cache.save(&self.key, &title) {
            Ok(report) => tracing::info!(
                links = report.links,
                bytes = report.bytes,
                working,
                complete,
                "sources saved for the next time"
            ),
            Err(error) => {
                tracing::warn!(%error, "cannot save the sources; playing goes on without them");
            }
        }
    }

    fn forget_if_dead(&self, complete: bool) {
        if !complete {
            tracing::debug!("nothing works and the search did not finish; keeping what was saved");
            return;
        }
        match self.cache.forget(&self.key) {
            Ok(true) => tracing::info!("the saved sources no longer work; removed them"),
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "cannot remove dead saved sources"),
        }
    }
}

#[cfg(test)]
#[path = "link_ledger_tests.rs"]
mod tests;
