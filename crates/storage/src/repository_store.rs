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
    pub fn add(&self, entry: &RepositoryEntry) -> Result<(), StorageError> {
        self.database.with("add repository", |connection| {
            connection
                .execute(
                    "INSERT OR IGNORE INTO repositories (url, name, added_at) VALUES (?1, ?2, ?3)",
                    params![entry.url, entry.name, entry.added_at_ms],
                )
                .map(drop)
        })
    }

    /// Lists repositories in the order they were added.
    pub fn list(&self) -> Result<Vec<RepositoryEntry>, StorageError> {
        self.database.with("list repositories", |connection| {
            let mut statement = connection
                .prepare("SELECT url, name, added_at FROM repositories ORDER BY added_at, rowid")?;
            let rows = statement.query_map([], |row| {
                Ok(RepositoryEntry {
                    url: row.get(0)?,
                    name: row.get(1)?,
                    added_at_ms: row.get(2)?,
                })
            })?;
            rows.collect()
        })
    }

    /// Removes a repository by URL.
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
    fn remove_deletes_only_the_given_repository() {
        let store = store();
        store.add(&entry("a", 1)).expect("add");
        store.add(&entry("b", 2)).expect("add");
        store.remove("a").expect("remove");
        assert_eq!(store.list().expect("list")[0].url, "b");
    }
}
