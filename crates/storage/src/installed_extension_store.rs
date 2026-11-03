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
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn upsert(&self, extension: &InstalledExtension) -> Result<(), StorageError> {
        self.database
            .with("save installed extension", |connection| {
                connection
                    .execute(
                        "INSERT INTO installed_extensions
                         (file_name, internal_name, repository_url, version, trusted, icon_url)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(file_name) DO UPDATE SET
                         internal_name = excluded.internal_name,
                         repository_url = excluded.repository_url,
                         version = excluded.version,
                         trusted = excluded.trusted,
                         icon_url = COALESCE(excluded.icon_url, installed_extensions.icon_url)",
                        params![
                            extension.file_name,
                            extension.internal_name,
                            extension.repository_url,
                            extension.version,
                            extension.trusted,
                            extension.icon_url
                        ],
                    )
                    .map(drop)
            })
    }

    /// Lists installed extensions by file name.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn list(&self) -> Result<Vec<InstalledExtension>, StorageError> {
        self.database
            .with("list installed extensions", |connection| {
                let mut statement = connection.prepare(
                    "SELECT file_name, internal_name, repository_url, version, trusted, icon_url
                 FROM installed_extensions ORDER BY file_name",
                )?;
                let rows = statement.query_map([], |row| {
                    Ok(InstalledExtension {
                        file_name: row.get(0)?,
                        internal_name: row.get(1)?,
                        repository_url: row.get(2)?,
                        version: row.get(3)?,
                        trusted: row.get(4)?,
                        icon_url: row
                            .get::<_, Option<String>>(5)?
                            .filter(|icon| !icon.is_empty()),
                    })
                })?;
                rows.collect()
            })
    }

    /// Remembers an icon for installed extensions that do not have one yet.
    #[tracing::instrument(level = "trace", skip_all, fields(internal_name = ?internal_name), err)]
    pub fn set_icon_if_missing(&self, internal_name: &str, icon: &str) -> Result<(), StorageError> {
        self.database.with("save extension icon", |connection| {
            connection
                .execute(
                    "UPDATE installed_extensions SET icon_url = ?2
                     WHERE internal_name = ?1 AND (icon_url IS NULL OR icon_url = '')",
                    params![internal_name, icon],
                )
                .map(drop)
        })
    }

    /// Forgets an installed extension.
    #[tracing::instrument(level = "trace", skip_all, fields(file_name = ?file_name), err)]
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
            icon_url: None,
        }
    }

    fn store() -> InstalledExtensionStore {
        InstalledExtensionStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
    }

    #[test]
    fn an_icon_is_added_once_and_an_update_without_one_keeps_it() {
        let store = store();
        store.upsert(&extension("a.cs3", 1, false)).expect("insert");
        store
            .set_icon_if_missing("Provider", "https://i/a.png")
            .expect("icon");
        store
            .set_icon_if_missing("Provider", "https://i/other.png")
            .expect("kept");
        store
            .upsert(&extension("a.cs3", 2, false))
            .expect("update without an icon");
        let listed = store.list().expect("list");
        assert_eq!(listed[0].icon_url.as_deref(), Some("https://i/a.png"));
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
