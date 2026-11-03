use std::sync::Arc;

use core_model::RepositoryEntry;
use rusqlite::params;

use crate::{Database, StorageError};

/// Extension repositories the user added.
#[derive(Clone)]
pub struct RepositoryStore {
    database: Arc<Database>,
}

impl RepositoryStore {
    /// Creates a store over a shared database.
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    /// Adds a repository; adding the same URL again keeps the original entry.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn add(&self, entry: &RepositoryEntry) -> Result<(), StorageError> {
        self.database.with("add repository", |connection| {
            connection
                .execute(
                    "INSERT OR IGNORE INTO repositories (url, name, added_at, icon_url)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![entry.url, entry.name, entry.added_at_ms, entry.icon_url],
                )
                .map(drop)
        })
    }

    /// Lists repositories in the order they were added.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn list(&self) -> Result<Vec<RepositoryEntry>, StorageError> {
        self.database.with("list repositories", |connection| {
            let mut statement = connection.prepare(
                "SELECT url, name, added_at, icon_url FROM repositories ORDER BY added_at, rowid",
            )?;
            let rows = statement.query_map([], |row| {
                let icon: Option<String> = row.get(3)?;
                Ok(RepositoryEntry {
                    url: row.get(0)?,
                    name: row.get(1)?,
                    added_at_ms: row.get(2)?,
                    icon_url: icon.filter(|icon| !icon.is_empty()),
                })
            })?;
            rows.collect()
        })
    }

    /// Remembers the icon of a repository, or that it has none (`None`), so it is not looked up again.
    #[tracing::instrument(level = "trace", skip_all, fields(url = ?url), err)]
    pub fn set_icon(&self, url: &str, icon: Option<&str>) -> Result<(), StorageError> {
        self.database.with("save repository icon", |connection| {
            connection
                .execute(
                    "UPDATE repositories SET icon_url = ?2 WHERE url = ?1",
                    params![url, icon.unwrap_or_default()],
                )
                .map(drop)
        })
    }

    /// Addresses of repositories whose icon has not been looked up yet.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn urls_without_icon_info(&self) -> Result<Vec<String>, StorageError> {
        self.database
            .with("list repositories without icon", |connection| {
                let mut statement = connection.prepare(
                    "SELECT url FROM repositories WHERE icon_url IS NULL ORDER BY added_at, rowid",
                )?;
                let rows = statement.query_map([], |row| row.get(0))?;
                rows.collect()
            })
    }

    /// Removes a repository by URL.
    #[tracing::instrument(level = "trace", skip_all, fields(url = ?url), err)]
    pub fn remove(&self, url: &str) -> Result<(), StorageError> {
        self.database.with("remove repository", |connection| {
            connection
                .execute("DELETE FROM repositories WHERE url = ?1", params![url])
                .map(drop)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str, added_at_ms: i64) -> RepositoryEntry {
        RepositoryEntry {
            url: url.to_owned(),
            name: format!("repo {url}"),
            added_at_ms,
            icon_url: None,
        }
    }

    fn store() -> RepositoryStore {
        RepositoryStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
    }

    #[test]
    fn list_returns_repositories_in_added_order() {
        let store = store();
        store.add(&entry("b", 2)).expect("add");
        store.add(&entry("a", 1)).expect("add");
        let urls: Vec<_> = store
            .list()
            .expect("list")
            .into_iter()
            .map(|e| e.url)
            .collect();
        assert_eq!(urls, ["a", "b"]);
    }

    #[test]
    fn adding_the_same_url_twice_keeps_one_entry() {
        let store = store();
        store.add(&entry("a", 1)).expect("add");
        store.add(&entry("a", 9)).expect("add again");
        assert_eq!(store.list().expect("list").len(), 1);
    }

    #[test]
    fn a_repository_icon_is_kept_and_a_missing_one_is_remembered_as_missing() {
        let store = store();
        store.add(&entry("a", 1)).expect("add");
        store.add(&entry("b", 2)).expect("add");
        assert_eq!(store.urls_without_icon_info().expect("pending"), ["a", "b"]);
        store.set_icon("a", Some("https://i/a.png")).expect("icon");
        store.set_icon("b", None).expect("none");
        let icons: Vec<Option<String>> = store
            .list()
            .expect("list")
            .into_iter()
            .map(|e| e.icon_url)
            .collect();
        assert_eq!(icons, [Some("https://i/a.png".to_owned()), None]);
    }

    #[test]
    fn repositories_stored_before_icons_existed_are_listed_as_unchecked() {
        let store = store();
        store
            .database
            .with("legacy row", |connection| {
                connection
                    .execute(
                        "INSERT INTO repositories (url, name, added_at) VALUES ('old', 'Old', 1)",
                        [],
                    )
                    .map(drop)
            })
            .expect("insert");
        assert_eq!(store.urls_without_icon_info().expect("pending"), ["old"]);
    }

    #[test]
    fn remove_deletes_only_the_given_repository() {
        let store = store();
        store.add(&entry("a", 1)).expect("add");
        store.add(&entry("b", 2)).expect("add");
        store.remove("a").expect("remove");
        assert_eq!(store.list().expect("list")[0].url, "b");
    }
}
