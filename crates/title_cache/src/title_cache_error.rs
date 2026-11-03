//! Why the saved sources of a title could not be written.

use std::io;
use std::path::PathBuf;

/// A failure to save, change or remove saved sources. Reading never fails: a file that cannot be
/// read counts as not saved (see [`crate::Lookup`]).
#[derive(Debug, thiserror::Error)]
pub enum TitleCacheError {
    /// The disk refused: no space, no permission, a locked file.
    #[error("cannot {action} {}: {source}", path.display())]
    Disk {
        /// What was being done.
        action: &'static str,
        /// The file or folder involved.
        path: PathBuf,
        /// The reason the system gave.
        #[source]
        source: io::Error,
    },
    /// The links could not be turned into text.
    #[error("cannot encode the saved sources: {0}")]
    Encode(#[source] serde_json::Error),
    /// The file would be bigger than allowed, so nothing was written.
    #[error("the saved sources take {bytes} bytes, over the limit of {limit}")]
    TooLarge {
        /// Size the file would have had.
        bytes: u64,
        /// Size allowed.
        limit: u64,
    },
}

impl TitleCacheError {
    pub(crate) fn disk(action: &'static str, path: &std::path::Path, source: io::Error) -> Self {
        Self::Disk {
            action,
            path: path.to_path_buf(),
            source,
        }
    }
}
