//! What is remembered about a title: its stream links, how each one last checked out, and which
//! one played last.

use core_model::PlayableLink;
use serde::{Deserialize, Serialize};

/// How a link last checked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkHealth {
    /// A check found it playable.
    Working,
    /// A check or a playback attempt found it broken.
    Broken,
    /// Never checked.
    #[default]
    Unchecked,
}

/// A stream link and what is known about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedLink {
    /// The link as the provider gave it.
    pub link: PlayableLink,
    /// How it last checked out.
    #[serde(default)]
    pub health: LinkHealth,
}

impl CachedLink {
    /// A link nobody has checked yet.
    pub fn unchecked(link: PlayableLink) -> Self {
        Self {
            link,
            health: LinkHealth::Unchecked,
        }
    }

    /// Working links first, then unchecked ones, broken ones last.
    fn health_rank(&self) -> u8 {
        match self.health {
            LinkHealth::Working => 0,
            LinkHealth::Unchecked => 1,
            LinkHealth::Broken => 2,
        }
    }
}

/// The content saved for one title.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CachedTitle {
    /// Whether the search that produced the links had finished. A partial list is refreshed soon.
    #[serde(default)]
    pub complete: bool,
    /// The links, best first once normalized.
    #[serde(default)]
    pub links: Vec<CachedLink>,
    /// The address of the link that played well last time.
    #[serde(default)]
    pub last_good: Option<String>,
}

impl CachedTitle {
    /// The links in the order to try them: the last good one, then working ones, then unchecked
    /// ones, broken ones last; within a group the higher quality first. Each address appears once.
    pub fn playing_order(&self) -> Vec<&CachedLink> {
        let mut seen = std::collections::HashSet::new();
        let mut ordered: Vec<&CachedLink> = self
            .links
            .iter()
            .filter(|cached| seen.insert(cached.link.url.as_str()))
            .collect();
        ordered.sort_by_key(|cached| {
            let is_last_good = self.last_good.as_deref() == Some(cached.link.url.as_str());
            (
                !(is_last_good && cached.health != LinkHealth::Broken),
                cached.health_rank(),
                std::cmp::Reverse(cached.link.quality),
            )
        });
        ordered
    }

    /// The same title with the links put in playing order and cut to `max_links`.
    pub(crate) fn normalized(&self, max_links: usize) -> Self {
        let links = self
            .playing_order()
            .into_iter()
            .take(max_links)
            .cloned()
            .collect::<Vec<_>>();
        let last_good = self
            .last_good
            .clone()
            .filter(|url| links.iter().any(|cached| cached.link.url == *url));
        Self {
            complete: self.complete,
            links,
            last_good,
        }
    }

    /// Notes how the link at `url` checked out; ignored when the title has no such link.
    pub fn set_health(&mut self, url: &str, health: LinkHealth) {
        for cached in self
            .links
            .iter_mut()
            .filter(|cached| cached.link.url == url)
        {
            cached.health = health;
        }
        if health == LinkHealth::Broken && self.last_good.as_deref() == Some(url) {
            self.last_good = None;
        }
    }

    /// Notes that the link at `url` played well.
    pub fn set_played_well(&mut self, url: &str) {
        self.set_health(url, LinkHealth::Working);
        if self.links.iter().any(|cached| cached.link.url == url) {
            self.last_good = Some(url.to_owned());
        }
    }
}

/// Whether saved links can be played at once or should be tested first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Saved a short while ago: links are likely still valid.
    Fresh,
    /// Saved a while ago, or the clock went backwards: test the links before relying on them.
    Stale,
}

#[cfg(test)]
#[path = "cached_title_tests.rs"]
mod tests;
