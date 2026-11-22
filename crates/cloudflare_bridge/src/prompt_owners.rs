//! Which request speaks for an origin on the viewer's screen.
//!
//! A page asks one site for many things at once. The viewer must be asked once per site, and the
//! banner must show one check per site, so the first request of an origin owns the prompt and the
//! requests that follow are quiet: they are still checked and answered, but raise no events.

use std::collections::{HashMap, HashSet, VecDeque};

/// What a request is for the prompt of its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// It speaks for its origin: its events reach the banner.
    Owner,
    /// An earlier request already speaks for the origin.
    Quiet,
}

struct Group {
    owner: String,
    followers: VecDeque<String>,
}

/// The prompt owner of every origin that has a check going.
#[derive(Default)]
pub(crate) struct PromptOwners {
    groups: HashMap<String, Group>,
    origin_of: HashMap<String, String>,
    quiet: HashSet<String>,
}

impl PromptOwners {
    pub(crate) fn has_owner(&self, origin: &str) -> bool {
        self.groups.contains_key(origin)
    }

    pub(crate) fn is_quiet(&self, id: &str) -> bool {
        self.quiet.contains(id)
    }

    /// `id` is a request for `origin`: the first one owns the prompt, later ones are quiet.
    pub(crate) fn join(&mut self, origin: &str, id: &str) -> Role {
        self.origin_of.insert(id.to_owned(), origin.to_owned());
        match self.groups.get_mut(origin) {
            Some(group) => {
                group.followers.push_back(id.to_owned());
                self.quiet.insert(id.to_owned());
                Role::Quiet
            }
            None => {
                let group = Group {
                    owner: id.to_owned(),
                    followers: VecDeque::new(),
                };
                self.groups.insert(origin.to_owned(), group);
                Role::Owner
            }
        }
    }

    /// `id` has its final answer. When it was the owner the group ends; its followers stay quiet until
    /// their own answers, and a later request of the origin starts a new group of its own.
    pub(crate) fn finish(&mut self, id: &str) {
        let Some(origin) = self.origin_of.remove(id) else {
            return;
        };
        self.quiet.remove(id);
        match self.groups.get_mut(&origin) {
            Some(group) if group.owner == id => {
                self.groups.remove(&origin);
            }
            Some(group) => group.followers.retain(|follower| follower != id),
            None => {}
        }
    }

    /// `id` left while the check goes on (the provider withdrew it). When it owned the prompt, the oldest
    /// follower takes over and is returned.
    pub(crate) fn withdraw(&mut self, id: &str) -> Option<String> {
        let origin = self.origin_of.remove(id)?;
        self.quiet.remove(id);
        let group = self.groups.get_mut(&origin)?;
        if group.owner != id {
            group.followers.retain(|follower| follower != id);
            return None;
        }
        let Some(next) = group.followers.pop_front() else {
            self.groups.remove(&origin);
            return None;
        };
        group.owner.clone_from(&next);
        self.quiet.remove(&next);
        Some(next)
    }

    /// The viewer dismissed the owner's prompt: the whole group is dismissed with it. Returns the
    /// followers, still quiet, for the caller to retire.
    pub(crate) fn dissolve(&mut self, id: &str) -> Vec<String> {
        let Some(origin) = self.origin_of.get(id) else {
            return Vec::new();
        };
        if self
            .groups
            .get(origin)
            .is_none_or(|group| group.owner != id)
        {
            return Vec::new();
        }
        let group = self.groups.remove(origin.as_str());
        group.map_or_else(Vec::new, |group| group.followers.into())
    }
}

#[cfg(test)]
#[path = "prompt_owners_tests.rs"]
mod tests;
