//! Everything that can go wrong while checking for or installing an update. The two commands turn it
//! into text for the page only at the very end; inside the updater it stays typed.

use std::io;
use std::path::PathBuf;

/// Why an update check or an update failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum UpdateError {
    /// The latest release is not newer than the running version (it changed since the check).
    #[error("There is no newer version to install (the latest is {latest}).")]
    NotNewer {
        /// The latest release's tag.
        latest: String,
    },
    /// The latest release has no file for this system.
    #[error("The latest release has no file for this system.")]
    NoAsset,
    /// The release has no signed checksum list, so its files cannot be checked.
    #[error(
        "The latest release is not signed, so it was not installed. Download it from GitHub if you trust it."
    )]
    NotSigned,
    /// This build carries no valid key to check releases with.
    #[error(
        "This version cannot check updates, so it does not install them. Download the new version from GitHub. ({0})"
    )]
    NoUpdateKey(#[source] minisign_verify::Error),
    /// The checksum list's signature is not valid for the built-in key: the list was changed or
    /// signed by someone else.
    #[error("The release's signature is not valid, so the update was not installed: {0}")]
    Signature(#[source] minisign_verify::Error),
    /// The signed checksum list has a line that is not `<sha256> <file name>`.
    #[error("The release's checksum list cannot be read: {0}")]
    ChecksumList(String),
    /// The signed checksum list does not name the file chosen for this system.
    #[error("The release's checksum list has no entry for '{file}', so it was not installed.")]
    MissingChecksum {
        /// The release file's name.
        file: String,
    },
    /// The downloaded file is not the one the release was signed for.
    #[error(
        "The download of '{file}' does not match the signed release, so it was not installed."
    )]
    ChecksumMismatch {
        /// The release file's name.
        file: String,
    },
    /// A request to GitHub failed (no connection, timeout, an error status).
    #[error("Could not {what}: {source}")]
    Request {
        /// What the request was for, e.g. "check for updates".
        what: &'static str,
        /// The HTTP client's error.
        #[source]
        source: Box<ureq::Error>,
    },
    /// GitHub answered with release information that could not be read.
    #[error("Could not read the release information: {0}")]
    Metadata(#[source] serde_json::Error),
    /// The connection broke while the update was downloading.
    #[error("The download stopped: {0}")]
    Stream(#[source] io::Error),
    /// A file or folder could not be created, written, moved or found.
    #[error("Could not {action} '{}': {source}", path.display())]
    Disk {
        /// What was being done, e.g. "create".
        action: &'static str,
        /// The file or folder.
        path: PathBuf,
        /// The system's error.
        #[source]
        source: io::Error,
    },
    /// The release's tag is not a version number, so it cannot be compared and is not offered.
    #[error("The release tag '{tag}' is not a version number: {source}")]
    UnreadableVersion {
        /// The tag as GitHub gave it.
        tag: String,
        /// Why it could not be read.
        #[source]
        source: semver::Error,
    },
    /// The system could not say where the running program is, so there is nothing to replace.
    #[error("Could not find the running program: {0}")]
    RunningProgramUnknown(#[source] io::Error),
    /// The program sits in a folder this user cannot write to, so it cannot be replaced.
    #[error(
        "Cannot update: destination '{}' is read-only. Please place the executable in a writable folder.",
        .0.display()
    )]
    ReadOnlyTarget(PathBuf),
    /// The helper that swaps the files after the app closes could not be started.
    #[error("Could not start the update helper: {0}")]
    Spawn(#[source] io::Error),
}

impl UpdateError {
    /// A failed request to GitHub.
    pub(crate) fn request(what: &'static str, source: ureq::Error) -> Self {
        Self::Request {
            what,
            source: Box::new(source),
        }
    }

    /// A failed file operation on `path`.
    pub(crate) fn disk(action: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Disk {
            action,
            path: path.into(),
            source,
        }
    }
}
