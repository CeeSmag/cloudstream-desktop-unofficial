use std::sync::Arc;

use core_model::Bookmark;
use rusqlite::params;

use crate::{Database, StorageError};

/// The user's saved titles.
#[derive(Clone)]
pub struct BookmarkStore {
    database: Arc<Database>,
}

impl BookmarkStore {
    /// Creates a store over a shared database.
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    /// Saves a title; saving it again refreshes its metadata but keeps the original date.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn add(&self, bookmark: &Bookmark) -> Result<(), StorageError> {
        self.database.with("add bookmark", |connection| {
            connection
                .execute(
                    "INSERT INTO bookmarks (provider, url, name, poster_url, media_type, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(provider, url) DO UPDATE SET
                         name = excluded.name, poster_url = excluded.poster_url,
                         media_type = excluded.media_type",
                    params![
                        bookmark.provider,
                        bookmark.url,
                        bookmark.name,
                        bookmark.poster_url,
                        bookmark.media_type,
                        bookmark.added_at_ms
                    ],
                )
                .map(drop)
        })
    }

    /// Removes a saved title.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, url = ?url), err)]
    pub fn remove(&self, provider: &str, url: &str) -> Result<(), StorageError> {
        self.database.with("remove bookmark", |connection| {
            connection
                .execute(
                    "DELETE FROM bookmarks WHERE provider = ?1 AND url = ?2",
                    params![provider, url],
                )
                .map(drop)
        })
    }

    /// Whether a title is saved.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, url = ?url), err)]
    pub fn contains(&self, provider: &str, url: &str) -> Result<bool, StorageError> {
        self.database.with("check bookmark", |connection| {
            connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM bookmarks WHERE provider = ?1 AND url = ?2)",
                params![provider, url],
                |row| row.get(0),
            )
        })
    }

    /// Lists saved titles, newest first.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn list(&self) -> Result<Vec<Bookmark>, StorageError> {
        self.database.with("list bookmarks", |connection| {
            let mut statement = connection.prepare(
                "SELECT provider, url, name, poster_url, media_type, added_at
                 FROM bookmarks ORDER BY added_at DESC, name",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(Bookmark {
                    provider: row.get(0)?,
                    url: row.get(1)?,
                    name: row.get(2)?,
                    poster_url: row.get(3)?,
                    media_type: row.get(4)?,
                    added_at_ms: row.get(5)?,
                })
            })?;
            rows.collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bookmark(url: &str, added_at_ms: i64) -> Bookmark {
        Bookmark {
            provider: "P".to_owned(),
            url: url.to_owned(),
            name: format!("title {url}"),
            added_at_ms,
            ..Bookmark::default()
        }
    }

    fn store() -> BookmarkStore {
        BookmarkStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
    }

    #[test]
    fn list_returns_newest_first() {
        let store = store();
        store.add(&bookmark("old", 1)).expect("add");
        store.add(&bookmark("new", 2)).expect("add");
        assert_eq!(store.list().expect("list")[0].url, "new");
    }

    #[test]
    fn contains_follows_add_and_remove() {
        let store = store();
        assert!(!store.contains("P", "u").expect("check"));
        store.add(&bookmark("u", 1)).expect("add");
        assert!(store.contains("P", "u").expect("check"));
        store.remove("P", "u").expect("remove");
        assert!(!store.contains("P", "u").expect("check"));
    }

    #[test]
    fn re_adding_keeps_the_original_date() {
        let store = store();
        store.add(&bookmark("u", 1)).expect("add");
        store.add(&bookmark("u", 99)).expect("add again");
        assert_eq!(store.list().expect("list")[0].added_at_ms, 1);
    }
}
