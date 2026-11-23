//! Plugins allowed although their listing does not say "video".
//!
//! Repository labels cannot tell a video site from a game: YouTube and Doom are both `Others`. The
//! rule denies those labels, and this list names the plugins a person checked and found to serve
//! video (from the listing's description and repository, census of Nov 2026), plus MegaProvider,
//! the one non-content plugin the app keeps because it brings in the repositories.
//!
//! An entry matches one repository ([`RepositoryId`], however its address is written) and one
//! internal name, so a same-named plugin elsewhere is not let in. Add entries only after checking
//! the plugin; each needs a reason.

use crate::repository_id::RepositoryId;

/// One reviewed plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewedPlugin {
    /// [`RepositoryId`] text of the repository that publishes it.
    pub repository: &'static str,
    /// The plugin's `internalName`, exactly.
    pub internal_name: &'static str,
    /// Why it is allowed.
    pub reason: &'static str,
}

/// Allowed after review.
pub const REVIEWED: &[ReviewedPlugin] = &[
    entry(
        "github.com/bnyro/germanproviders",
        "C3TV",
        "Video talks from media.ccc.de",
    ),
    entry(
        "github.com/cranberrysoup/aniyomicompatextension",
        "AniyomiProvider",
        "Anime video through Aniyomi extensions",
    ),
    entry(
        "gitlab.com/tearrs/cloudstream-vietnamese",
        "MonPlayerProvider",
        "Plays the video links pinned in its Telegram group",
    ),
    entry(
        "github.com/faisal0786/desi",
        "AryaVidya",
        "Lecture and OTT videos",
    ),
    entry(
        "github.com/kraptor123/cs-karma",
        "FootReplays",
        "Football match replays",
    ),
    entry(
        "github.com/redowan99/redowan-cloudstream",
        "FootReplays",
        "Football replays and highlights",
    ),
    entry(
        "github.com/redowan99/redowan-cloudstream",
        "FullReplays",
        "Full football match replays",
    ),
    entry(
        "github.com/luna712/luna712-cloudstream-extensions",
        "DailymotionProvider",
        "Videos from Dailymotion",
    ),
    entry(
        "github.com/luna712/luna712-cloudstream-extensions",
        "InternetArchiveProvider",
        "Videos from archive.org",
    ),
    entry(
        "github.com/recloudstream/extensions",
        "DailymotionProvider",
        "Videos from Dailymotion",
    ),
    entry(
        "github.com/recloudstream/extensions",
        "InternetArchiveProvider",
        "Videos from archive.org",
    ),
    entry(
        "github.com/recloudstream/extensions",
        "InvidiousProvider",
        "YouTube videos through Invidious",
    ),
    entry(
        "github.com/dogior/dogiorshadenough",
        "Nebula",
        "Free videos from nebula.tv",
    ),
    entry(
        "github.com/dogior/dogiorshadenough",
        "YouTube",
        "Videos, playlists and channels from YouTube",
    ),
    entry(
        "github.com/techtanic/skillshare-repo",
        "SkillShareProvider",
        "Skillshare video courses",
    ),
    entry(
        "github.com/self-similarity/megarepo",
        "MegaProvider",
        "No content: adds the repositories of the other plugins",
    ),
];

const fn entry(
    repository: &'static str,
    internal_name: &'static str,
    reason: &'static str,
) -> ReviewedPlugin {
    ReviewedPlugin {
        repository,
        internal_name,
        reason,
    }
}

/// The entry of `list` for this plugin, if any.
#[must_use]
pub fn find_reviewed(
    list: &'static [ReviewedPlugin],
    repository_url: &str,
    internal_name: &str,
) -> Option<&'static ReviewedPlugin> {
    let repository = RepositoryId::of(repository_url);
    list.iter().find(|entry| {
        entry.internal_name == internal_name && entry.repository == repository.as_str()
    })
}

#[cfg(test)]
#[path = "review_list_tests.rs"]
mod tests;
