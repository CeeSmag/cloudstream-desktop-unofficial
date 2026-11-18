//! Opening a video that was opened before: what the saved sources allow.
//!
//! Saved a short while ago, the best ones are put in the list, the best of them plays at once and
//! they are checked again in the background. Saved a while ago, the best three are put in the list
//! and tested first, and the first that works plays. Either way the provider's search still runs
//! and refreshes what is saved. Nothing saved (or nothing usable) means the normal wait.

use std::time::Duration;

use app_services::link_rank;
use core_model::PlayableLink;
use title_cache::{CachedLink, CachedTitle, Freshness, LinkHealth, Lookup, TitleCache};

use crate::check_gate::CheckGate;
use crate::link_ledger::LinkLedger;
use crate::source_session::SourceSession;

/// How many saved streams are put in the list and checked again when they are trusted.
const TRUSTED_RECHECK: usize = 8;
/// How many saved streams are tested before one of them plays.
const TESTED_FIRST: usize = 3;
/// How long the test of saved streams may take before the video waits for the search instead.
pub(crate) const TEST_CAP: Duration = Duration::from_secs(3);

/// How the saved sources are used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SavedMode {
    /// Nothing usable was saved.
    Nothing,
    /// Saved recently: play the best at once.
    Trusted,
    /// Saved a while ago: test the best few first.
    Testing,
}

/// The saved sources of a title, ready to be put in the list of a video that is opening.
pub(crate) struct SavedStart {
    pub(crate) mode: SavedMode,
    /// The streams to put in the list, best first.
    pub(crate) seeds: Vec<CachedLink>,
    /// The rest of what was saved.
    pub(crate) rest: CachedTitle,
}

impl SavedStart {
    fn nothing() -> Self {
        Self {
            mode: SavedMode::Nothing,
            seeds: Vec::new(),
            rest: CachedTitle::default(),
        }
    }

    /// Looks up the title. Problems with the files mean nothing was saved.
    pub(crate) fn find(cache: &TitleCache, title_key: &str) -> Self {
        let Lookup::Hit(hit) = cache.load(title_key) else {
            return Self::nothing();
        };
        let (mode, wanted) = match hit.freshness {
            Freshness::Fresh => (SavedMode::Trusted, TRUSTED_RECHECK),
            Freshness::Stale => (SavedMode::Testing, TESTED_FIRST),
        };
        let seeds: Vec<CachedLink> = hit
            .title
            .playing_order()
            .into_iter()
            .filter(|cached| cached.health != LinkHealth::Broken)
            .take(wanted)
            .cloned()
            .collect();
        let rest = CachedTitle {
            links: hit
                .title
                .links
                .iter()
                .filter(|cached| !seeds.iter().any(|seed| seed.link.url == cached.link.url))
                .cloned()
                .collect(),
            ..hit.title
        };
        tracing::info!(
            ?mode,
            seeds = seeds.len(),
            saved = seeds.len() + rest.links.len(),
            age_secs = hit.age.map(|age| age.as_secs()),
            "found sources saved for this title"
        );
        if seeds.is_empty() {
            return Self {
                mode: SavedMode::Nothing,
                seeds,
                rest,
            };
        }
        Self { mode, seeds, rest }
    }

    /// Puts the saved streams in the list of the open video, tells the gate how to treat them and
    /// the ledger what they are. Returns the streams to check, with their places in the list.
    pub(crate) fn plant(
        &self,
        target: (&CheckGate, &SourceSession, u64),
        ledger: &LinkLedger,
    ) -> Vec<(usize, PlayableLink)> {
        let (gate, session, generation) = target;
        if self.seeds.is_empty() {
            return Vec::new();
        }
        let links: Vec<PlayableLink> = self
            .seeds
            .iter()
            .map(|cached| cached.link.clone())
            .collect();
        let Some(first) = session.append(generation, links.clone(), Vec::new()) else {
            return Vec::new();
        };
        ledger.plant(&self.seeds);
        let ranks: Vec<i32> = links.iter().map(link_rank).collect();
        gate.announce_saved(&ranks);
        match self.mode {
            SavedMode::Trusted => gate.trust_saved(first),
            SavedMode::Testing => gate.test_saved_first(links.len(), TEST_CAP),
            SavedMode::Nothing => gate.progress(),
        };
        links
            .into_iter()
            .enumerate()
            .map(|(offset, link)| (first + offset, link))
            .collect()
    }
}
