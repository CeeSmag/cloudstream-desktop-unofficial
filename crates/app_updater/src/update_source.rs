//! Where the updater reads releases from, and the key their checksum lists must be signed with.
//! The app always uses [`UpdateSource::OFFICIAL`]; the tests point it at a local server and at a
//! key made for the test, so the whole check-download-verify path runs without GitHub.

use crate::update_verify::UPDATE_SIGNING_KEY;

/// GitHub's list of this project's releases, newest first (pre-releases included).
const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/CeeSmag/cloudstream-desktop-unofficial/releases?per_page=10";

/// The release list to read and the public key that signs every release's checksum list.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UpdateSource<'a> {
    /// The address of the release list (GitHub's releases API answer format).
    pub(crate) releases_url: &'a str,
    /// The minisign public key file's text.
    pub(crate) signing_key: &'a str,
}

impl UpdateSource<'static> {
    /// This project's releases on GitHub, checked with the key built into the app.
    pub(crate) const OFFICIAL: Self = Self {
        releases_url: GITHUB_RELEASES_API,
        signing_key: UPDATE_SIGNING_KEY,
    };
}
