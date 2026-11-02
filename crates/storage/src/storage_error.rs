use std::path::PathBuf;

/// Why a storage operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The database file or its folder could not be prepared.
    #[error("cannot prepare database at {path}: {source}")]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying I/O failure.
        source: std::io::Error,
    },
    /// An SQL statement failed.
    #[error("database operation '{operation}' failed: {source}")]
    Sql {
        /// Short name of the operation.
        operation: &'static str,
        /// Underlying SQLite failure.
        source: rusqlite::Error,
    },
}

impl StorageError {
    /// Builds a closure that tags an SQLite failure with the operation that caused it.
    pub(crate) fn sql(operation: &'static str) -> impl Fn(rusqlite::Error) -> Self {
        move |source| Self::Sql { operation, source }
    }
}
