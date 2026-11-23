//! The content policy ([`content_policy`]) applied to repository listings and installed extensions.
//!
//! The app supports video plugins only. Facts (the `tvTypes` a listing declared) are stored with each
//! installed extension; verdicts are worked out from them each time, so a change of rule applies to
//! extensions already installed.

use content_policy::{ContentPolicy, PluginFacts, Verdict};
use core_model::{InstalledExtension, PluginListing, ProviderInfo};

/// Whether an installed extension is supported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Support {
    /// A video plugin: loaded and offered.
    Supported,
    /// Not a video plugin: never loaded; the viewer may uninstall it. The text says why.
    NotVideo(String),
    /// Installed before content types were recorded and not judged yet (it loads; the next
    /// start or catalog visit decides).
    Unknown,
}

/// An installed extension with its support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledStatus {
    /// The extension.
    pub extension: InstalledExtension,
    /// Whether the app supports it.
    pub support: Support,
}

/// The verdict for a listing of the repository at `repository_url`.
pub(crate) fn judge_listing(
    policy: &ContentPolicy,
    repository_url: &str,
    listing: &PluginListing,
) -> Verdict {
    policy.judge(&PluginFacts {
        repository_url,
        internal_name: &listing.internal_name,
        declared_types: &listing.tv_types,
    })
}

/// The verdict for an installed extension, or `None` while its content types are unknown.
pub(crate) fn judge_installed(
    policy: &ContentPolicy,
    extension: &InstalledExtension,
) -> Option<Verdict> {
    let declared_types = extension.declared_types.as_deref()?;
    Some(policy.judge(&PluginFacts {
        repository_url: extension.repository_url.as_deref().unwrap_or_default(),
        internal_name: &extension.internal_name,
        declared_types,
    }))
}

/// For an extension whose listing types are unknown: the verdict from the types its providers
/// report, and those types (to record, so it is not judged again).
pub(crate) fn judge_by_providers(
    policy: &ContentPolicy,
    extension: &InstalledExtension,
    providers: &[ProviderInfo],
) -> (Verdict, Vec<String>) {
    let mut types: Vec<String> = Vec::new();
    for label in providers.iter().flat_map(|provider| &provider.types) {
        if !types.contains(label) {
            types.push(label.clone());
        }
    }
    let verdict = policy.judge(&PluginFacts {
        repository_url: extension.repository_url.as_deref().unwrap_or_default(),
        internal_name: &extension.internal_name,
        declared_types: &types,
    });
    (verdict, types)
}

/// What the viewer sees for a verdict.
pub(crate) fn support_of(verdict: Option<&Verdict>) -> Support {
    match verdict {
        None => Support::Unknown,
        Some(Verdict::Video(_)) => Support::Supported,
        Some(Verdict::NotVideo(refusal)) => Support::NotVideo(refusal.to_string()),
    }
}

/// The name to show for a listing (its display name, else its internal name).
pub(crate) fn listing_label(listing: &PluginListing) -> &str {
    if listing.name.trim().is_empty() {
        &listing.internal_name
    } else {
        &listing.name
    }
}

#[cfg(test)]
#[path = "content_support_tests.rs"]
mod tests;
