use std::sync::Arc;

use core_model::InstalledExtension;
use rusqlite::params;

use crate::{Database, StorageError};

/// Extension files the user installed.
#[derive(Clone)]
pub struct InstalledExtensionStore {
    database: Arc<Database>,
}

impl InstalledExtensionStore {
    /// Creates a store over a shared database.
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    /// Inserts or updates an installed extension, keyed by file name.
    pub fn upsert(&self, extension: &InstalledExtension) -> Result<(), StorageError> {
        self.database
            .with("save installed extension", |connection| {
                connection
                    .execute(
                        "INSERT INTO installed_extensions
                         (file_name, internal_name, repository_url, version, trusted)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(file_name) DO UPDATE SET
                         internal_name = excluded.internal_name,
                         repository_url = excluded.repository_url,
                         version = excluded.version,
                         trusted = excluded.trusted",
                        params![
                            extension.file_name,
                            extension.internal_name,
                            extension.repository_url,
                            extension.version,
                            extension.trusted
                        ],
                    )
                    .map(drop)
            })
    }

    /// Lists installed extensions by file name.
    pub fn list(&self) -> Result<Vec<InstalledExtension>, StorageError> {
        self.database
            .with("list installed extensions", |connection| {
                let mut statement = connection.prepare(
                    "SELECT file_name, internal_name, repository_url, version, trusted
                 FROM installed_extensions ORDER BY file_name",
                )?;
                let rows = statement.query_map([], |row| {
                    Ok(InstalledExtension {
                        file_name: row.get(0)?,
                        internal_name: row.get(1)?,
                        repository_url: row.get(2)?,
                        version: row.get(3)?,
                        trusted: row.get(4)?,
                    })
                })?;
                rows.collect()
            })
    }

    /// Forgets an installed extension.
    pub fn remove(&self, file_name: &str) -> Result<(), StorageError> {
        self.database
            .with("remove installed extension", |connection| {
                connection
                    .execute(
                        "DELETE FROM installed_extensions WHERE file_name = ?1",
                        params![file_name],
                    )
                    .map(drop)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extension(file_name: &str, version: i64, trusted: bool) -> InstalledExtension {
        InstalledExtension {
            file_name: file_name.to_owned(),
            internal_name: "Provider".to_owned(),
            repository_url: Some("https://r.test/repo.json".to_owned()),
            version,
            trusted,
        }
    }

    fn store() -> InstalledExtensionStore {
        InstalledExtensionStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
    }

    #[test]
    fn upsert_updates_version_and_trust_for_the_same_file() {
        let store = store();
        store.upsert(&extension("a.cs3", 1, false)).expect("insert");
        store.upsert(&extension("a.cs3", 2, true)).expect("update");
        assert_eq!(
            store.list().expect("list"),
            vec![extension("a.cs3", 2, true)]
        );
    }

    #[test]
    fn remove_forgets_the_extension() {
        let store = store();
        store.upsert(&extension("a.cs3", 1, false)).expect("insert");
        store.remove("a.cs3").expect("remove");
        assert!(store.list().expect("list").is_empty());
    }
}
