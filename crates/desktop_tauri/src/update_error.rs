//! Everything that can go wrong while checking for or installing an update. The two commands turn it
//! into text for the page only at the very end; inside the updater it stays typed.

use std::io;
use std::path::PathBuf;

/// Why an update check or an update failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum UpdateError {
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
