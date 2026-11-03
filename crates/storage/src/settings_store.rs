use std::sync::Arc;

use rusqlite::{params, OptionalExtension};

use crate::{Database, StorageError};

/// String key/value settings.
#[derive(Clone)]
pub struct SettingsStore {
    database: Arc<Database>,
}

impl SettingsStore {
    /// Creates a store over a shared database.
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    /// Reads a setting.
    #[tracing::instrument(level = "trace", skip_all, fields(key = ?key), err)]
    pub fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.database.with("read setting", |connection| {
            connection
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()
        })
    }

    /// Writes a setting, replacing any previous value.
    #[tracing::instrument(level = "trace", skip_all, fields(key = ?key, value = ?value), err)]
    pub fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.database.with("write setting", |connection| {
            connection
                .execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )
                .map(drop)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SettingsStore {
        SettingsStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        assert_eq!(store().get("missing").expect("read"), None);
    }

    #[test]
    fn set_then_get_round_trips_and_overwrites() {
        let store = store();
        store.set("theme", "dark").expect("write");
        store.set("theme", "light").expect("overwrite");
        assert_eq!(store.get("theme").expect("read"), Some("light".to_owned()));
    }
}
