use std::path::PathBuf;

/// Why a repository or download operation failed.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    /// An HTTP request failed or returned an error status.
    #[error("cannot download {url}: {reason}")]
    Download {
        /// URL that failed.
        url: String,
        /// Short reason.
        reason: String,
    },
    /// A repository file was not valid JSON of the expected shape.
    #[error("invalid repository data from {url}: {reason}")]
    Invalid {
        /// URL the data came from.
        url: String,
        /// What was wrong.
        reason: String,
    },
    /// A plugin file could not be written or removed.
    #[error("cannot write {path}: {source}")]
    Disk {
        /// Path that failed.
        path: PathBuf,
        /// Underlying I/O failure.
        source: std::io::Error,
    },
    /// The plugin's internal name cannot be turned into a safe file name.
    #[error("unsafe plugin name '{0}'")]
    UnsafeName(String),
}
