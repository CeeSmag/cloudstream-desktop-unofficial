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
    /// An update keeps the original `installed_at_ms`, and keeps the declared types when it has none.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn upsert(&self, extension: &InstalledExtension) -> Result<(), StorageError> {
        self.database
            .with("save installed extension", |connection| {
                connection
                    .execute(
                        "INSERT INTO installed_extensions
                         (file_name, internal_name, repository_url, version, trusted, icon_url, installed_at,
                          declared_types)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(file_name) DO UPDATE SET
                         internal_name = excluded.internal_name,
                         repository_url = excluded.repository_url,
                         version = excluded.version,
                         trusted = excluded.trusted,
                         icon_url = COALESCE(excluded.icon_url, installed_extensions.icon_url),
                         installed_at = installed_extensions.installed_at,
                         declared_types = COALESCE(excluded.declared_types, installed_extensions.declared_types)",
                        params![
                            extension.file_name,
                            extension.internal_name,
                            extension.repository_url,
                            extension.version,
                            extension.trusted,
                            extension.icon_url,
                            extension.installed_at_ms,
                            extension.declared_types.as_deref().map(encode_types)
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
                    "SELECT file_name, internal_name, repository_url, version, trusted, icon_url, installed_at,
                        declared_types
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
                        installed_at_ms: row.get(6)?,
                        declared_types: row.get::<_, Option<String>>(7)?.and_then(|text| decode_types(&text)),
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

    /// Records the content types of an extension whose types are not known yet (rows from before
    /// they were recorded). Known types are never overwritten: the listing is the better source.
    #[tracing::instrument(level = "trace", skip_all, fields(file_name = ?file_name), err)]
    pub fn set_declared_types_if_missing(
        &self,
        file_name: &str,
        types: &[String],
    ) -> Result<(), StorageError> {
        self.database
            .with("save extension content types", |connection| {
                connection
                    .execute(
                        "UPDATE installed_extensions SET declared_types = ?2
                     WHERE file_name = ?1 AND declared_types IS NULL",
                        params![file_name, encode_types(types)],
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

fn encode_types(types: &[String]) -> String {
    serde_json::to_string(types).unwrap_or_else(|_| "[]".to_owned())
}

/// A value that is not a JSON list of strings is treated as unknown, so the policy asks again.
fn decode_types(text: &str) -> Option<Vec<String>> {
    match serde_json::from_str(text) {
        Ok(types) => Some(types),
        Err(error) => {
            tracing::warn!(%error, "unreadable declared content types; treating them as unknown");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extension(
        file_name: &str,
        version: i64,
        trusted: bool,
        installed_at_ms: i64,
    ) -> InstalledExtension {
        InstalledExtension {
            file_name: file_name.to_owned(),
            internal_name: "Provider".to_owned(),
            repository_url: Some("https://r.test/repo.json".to_owned()),
            version,
            trusted,
            icon_url: None,
            installed_at_ms,
            declared_types: None,
        }
    }

    fn store() -> InstalledExtensionStore {
        InstalledExtensionStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
    }

    #[test]
    fn an_icon_is_added_once_and_an_update_without_one_keeps_it() {
        let store = store();
        store
            .upsert(&extension("a.cs3", 1, false, 10))
            .expect("insert");
        store
            .set_icon_if_missing("Provider", "https://i/a.png")
            .expect("icon");
        store
            .set_icon_if_missing("Provider", "https://i/other.png")
            .expect("kept");
        store
            .upsert(&extension("a.cs3", 2, false, 99))
            .expect("update without an icon");
        let listed = store.list().expect("list");
        assert_eq!(listed[0].icon_url.as_deref(), Some("https://i/a.png"));
    }

    #[test]
    fn upsert_updates_version_and_trust_for_the_same_file() {
        let store = store();
        store
            .upsert(&extension("a.cs3", 1, false, 10))
            .expect("insert");
        store
            .upsert(&extension("a.cs3", 2, true, 99))
            .expect("update");
        assert_eq!(
            store.list().expect("list"),
            vec![extension("a.cs3", 2, true, 10)]
        );
    }

    #[test]
    fn upsert_keeps_the_original_install_time_on_update() {
        let store = store();
        store
            .upsert(&extension("a.cs3", 1, false, 1_000))
            .expect("insert");
        store
            .upsert(&extension("a.cs3", 2, false, 9_000))
            .expect("update");
        assert_eq!(store.list().expect("list")[0].installed_at_ms, 1_000);
    }

    #[test]
    fn declared_types_round_trip_and_survive_an_update_without_them() {
        let store = store();
        let with_types = InstalledExtension {
            declared_types: Some(vec!["Movie".to_owned(), "Movie,Anime".to_owned()]),
            ..extension("a.cs3", 1, false, 10)
        };
        store.upsert(&with_types).expect("insert");
        store
            .upsert(&extension("a.cs3", 2, false, 10))
            .expect("update without types");
        assert_eq!(
            store.list().expect("list")[0].declared_types,
            with_types.declared_types
        );
    }

    #[test]
    fn missing_types_are_filled_once_and_known_ones_are_kept() {
        let store = store();
        store
            .upsert(&extension("a.cs3", 1, false, 10))
            .expect("insert");
        assert_eq!(store.list().expect("list")[0].declared_types, None);
        let movie = vec!["Movie".to_owned()];
        store
            .set_declared_types_if_missing("a.cs3", &movie)
            .expect("fill");
        store
            .set_declared_types_if_missing("a.cs3", &["Others".to_owned()])
            .expect("kept");
        assert_eq!(store.list().expect("list")[0].declared_types, Some(movie));
    }

    #[test]
    fn unreadable_types_read_as_unknown() {
        assert_eq!(decode_types("not json"), None);
        assert_eq!(decode_types("[1, 2]"), None);
        assert_eq!(decode_types("[]"), Some(vec![]));
    }

    #[test]
    fn remove_forgets_the_extension() {
        let store = store();
        store
            .upsert(&extension("a.cs3", 1, false, 10))
            .expect("insert");
        store.remove("a.cs3").expect("remove");
        assert!(store.list().expect("list").is_empty());
    }
}
