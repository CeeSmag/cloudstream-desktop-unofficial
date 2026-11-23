//! The rule: a plugin is supported when it is a video plugin.

use std::fmt;

use crate::media_type::{parse_labels, MediaType};
use crate::review_list::{find_reviewed, ReviewedPlugin, REVIEWED};

/// What is known about a plugin: where it is published and what its listing says it carries.
#[derive(Debug, Clone, Copy)]
pub struct PluginFacts<'a> {
    /// The `repo.json` address it came from.
    pub repository_url: &'a str,
    /// Its `internalName`.
    pub internal_name: &'a str,
    /// Its `tvTypes`, as written.
    pub declared_types: &'a [String],
}

/// The decision about one plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Supported.
    Video(Allowed),
    /// Not supported: hidden, not installed, not loaded.
    NotVideo(Refusal),
}

/// Why a plugin is supported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Allowed {
    /// Its listing names these video types.
    ByTypes(Vec<MediaType>),
    /// A person checked it ([`crate::REVIEWED`]); the reason.
    Reviewed(&'static str),
}

/// Why a plugin is not supported: what its listing declares instead of video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The types it declares, none of them video.
    pub types: Vec<MediaType>,
    /// Labels that named no type.
    pub unrecognised: Vec<String>,
}

impl Verdict {
    /// True when the plugin is supported.
    #[must_use]
    pub const fn is_video(&self) -> bool {
        matches!(self, Self::Video(_))
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.types.is_empty() {
            let names: Vec<&str> = self.types.iter().map(|media| media.name()).collect();
            write!(f, "not a video plugin (it declares {})", names.join(", "))
        } else if !self.unrecognised.is_empty() {
            write!(
                f,
                "not a video plugin (it declares only \"{}\")",
                self.unrecognised.join("\", \"")
            )
        } else {
            write!(f, "not a video plugin (it declares no content type)")
        }
    }
}

/// Decides which plugins the app supports. Immutable; share it freely.
#[derive(Debug, Clone, Copy)]
pub struct ContentPolicy {
    reviewed: &'static [ReviewedPlugin],
}

impl Default for ContentPolicy {
    fn default() -> Self {
        Self::standard()
    }
}

impl ContentPolicy {
    /// The policy of the app: the video types plus [`REVIEWED`].
    #[must_use]
    pub const fn standard() -> Self {
        Self { reviewed: REVIEWED }
    }

    /// The same rule with another reviewed list (for tests).
    #[must_use]
    pub const fn with_reviewed(reviewed: &'static [ReviewedPlugin]) -> Self {
        Self { reviewed }
    }

    /// The verdict for one plugin. A reviewed entry wins; otherwise any video type allows it.
    #[must_use]
    pub fn judge(&self, plugin: &PluginFacts<'_>) -> Verdict {
        if let Some(entry) = self.reviewed_entry(plugin) {
            return Verdict::Video(Allowed::Reviewed(entry.reason));
        }
        let parsed = parse_labels(plugin.declared_types);
        let video: Vec<MediaType> = parsed
            .types
            .iter()
            .copied()
            .filter(|m| m.is_video())
            .collect();
        if video.is_empty() {
            Verdict::NotVideo(Refusal {
                types: parsed.types,
                unrecognised: parsed.unrecognised,
            })
        } else {
            Verdict::Video(Allowed::ByTypes(video))
        }
    }

    /// Whether to offer one provider of a supported plugin, from the types the provider itself
    /// reports. Providers that report nothing follow their plugin. A provider reporting only
    /// non-video types is hidden, unless its plugin was reviewed (YouTube's providers say `Others`).
    #[must_use]
    pub fn shows_provider(&self, plugin: &Verdict, provider_types: &[String]) -> bool {
        match plugin {
            Verdict::NotVideo(_) => false,
            Verdict::Video(Allowed::Reviewed(_)) => true,
            Verdict::Video(Allowed::ByTypes(_)) => {
                let parsed = parse_labels(provider_types);
                parsed.types.is_empty() || parsed.types.iter().any(|m| m.is_video())
            }
        }
    }

    fn reviewed_entry(&self, plugin: &PluginFacts<'_>) -> Option<&'static ReviewedPlugin> {
        find_reviewed(self.reviewed, plugin.repository_url, plugin.internal_name)
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
