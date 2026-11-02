use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::Connection;

use crate::schema_migrations::migrate;
use crate::StorageError;

/// A shared SQLite connection. Cheap to share behind an `Arc`.
pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    /// Opens (creating if needed) the database file and migrates its schema.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|source| StorageError::Io {
                path: folder.to_path_buf(),
                source,
            })?;
        }
        let connection = Connection::open(path).map_err(StorageError::sql("open database"))?;
        Self::prepared(connection)
    }

    /// Opens a throw-away in-memory database (tests, previews).
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection =
            Connection::open_in_memory().map_err(StorageError::sql("open memory database"))?;
        Self::prepared(connection)
    }

    fn prepared(connection: Connection) -> Result<Self, StorageError> {
        migrate(&connection).map_err(StorageError::sql("migrate schema"))?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Runs `body` with the connection, tagging failures with `operation`.
    pub(crate) fn with<R>(
        &self,
        operation: &'static str,
        body: impl FnOnce(&Connection) -> rusqlite::Result<R>,
    ) -> Result<R, StorageError> {
        let guard: MutexGuard<'_, Connection> = self
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        body(&guard).map_err(StorageError::sql(operation))
    }
}
