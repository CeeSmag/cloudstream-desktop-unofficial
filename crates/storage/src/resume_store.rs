use std::sync::Arc;

use core_model::ResumePointer;
use rusqlite::{params, OptionalExtension, Row};

use crate::{Database, StorageError};

/// Resume pointers: stores the episode to resume next for each show or movie.
#[derive(Clone)]
pub struct ResumeStore {
    database: Arc<Database>,
}

fn read_pointer(row: &Row<'_>) -> rusqlite::Result<ResumePointer> {
    Ok(ResumePointer {
        provider: row.get(0)?,
        detail_url: row.get(1)?,
        media_key: row.get(2)?,
        title: row.get(3)?,
        poster_url: row.get(4)?,
        season: row.get(5)?,
        episode: row.get(6)?,
        updated_at_ms: row.get(7)?,
    })
}

const SELECT_COLUMNS: &str =
    "provider, detail_url, media_key, title, poster_url, season, episode, updated_at";

impl ResumeStore {
    /// Creates a store over a shared database.
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    /// Saves a resume pointer, replacing any earlier pointer for the same show.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn save(&self, pointer: &ResumePointer) -> Result<(), StorageError> {
        self.database.with("save resume pointer", |connection| {
            connection
                .execute(
                    "INSERT OR REPLACE INTO resume_pointers
                         (provider, detail_url, media_key, title, poster_url,
                          season, episode, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        pointer.provider,
                        pointer.detail_url,
                        pointer.media_key,
                        pointer.title,
                        pointer.poster_url,
                        pointer.season,
                        pointer.episode,
                        pointer.updated_at_ms
                    ],
                )
                .map(drop)
        })
    }

    /// Reads the saved resume pointer for one show or movie.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
    pub fn get(
        &self,
        provider: &str,
        detail_url: &str,
    ) -> Result<Option<ResumePointer>, StorageError> {
        self.database.with("read resume pointer", |connection| {
            connection
                .query_row(
                    &format!(
                        "SELECT {SELECT_COLUMNS} FROM resume_pointers WHERE provider = ?1 AND detail_url = ?2"
                    ),
                    params![provider, detail_url],
                    read_pointer,
                )
                .optional()
        })
    }

    /// Forgets the resume pointer for one show or movie. Returns whether there was one.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
    pub fn remove(&self, provider: &str, detail_url: &str) -> Result<bool, StorageError> {
        self.database.with("remove resume pointer", |connection| {
            connection
                .execute(
                    "DELETE FROM resume_pointers WHERE provider = ?1 AND detail_url = ?2",
                    params![provider, detail_url],
                )
                .map(|rows| rows > 0)
        })
    }

    /// Lists the most recently updated resume pointers, newest first.
    #[tracing::instrument(level = "trace", skip_all, fields(limit = ?limit), err)]
    pub fn recent(&self, limit: u32) -> Result<Vec<ResumePointer>, StorageError> {
        self.database
            .with("list recent resume pointers", |connection| {
                let mut statement = connection.prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM resume_pointers ORDER BY updated_at DESC LIMIT ?1"
            ))?;
                let rows = statement.query_map(params![limit], read_pointer)?;
                rows.collect()
            })
    }

    /// Lists all resume pointers.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn all(&self) -> Result<Vec<ResumePointer>, StorageError> {
        self.database
            .with("list all resume pointers", |connection| {
                let mut statement = connection.prepare(&format!(
                    "SELECT {SELECT_COLUMNS} FROM resume_pointers ORDER BY updated_at DESC"
                ))?;
                let rows = statement.query_map([], read_pointer)?;
                rows.collect()
            })
    }

    /// Clears all resume pointers.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn clear(&self) -> Result<(), StorageError> {
        self.database
            .with("clear all resume pointers", |connection| {
                connection
                    .execute("DELETE FROM resume_pointers", [])
                    .map(drop)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pointer(detail_url: &str, media_key: &str, updated_at_ms: i64) -> ResumePointer {
        ResumePointer {
            provider: "P".to_owned(),
            detail_url: detail_url.to_owned(),
            media_key: media_key.to_owned(),
            title: format!("Title {detail_url}"),
            poster_url: Some("poster.png".to_owned()),
            season: Some(1),
            episode: Some(2),
            updated_at_ms,
        }
    }

    fn store() -> ResumeStore {
        ResumeStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
    }

    #[test]
    fn save_replaces_pointer_for_the_same_show() {
        let store = store();
        store
            .save(&pointer("https://show/1", "ep1", 100))
            .expect("save ep1");
        store
            .save(&pointer("https://show/1", "ep2", 200))
            .expect("save ep2");

        let saved = store
            .get("P", "https://show/1")
            .expect("get")
            .expect("present");
        assert_eq!(saved.media_key, "ep2");
        assert_eq!(saved.updated_at_ms, 200);
    }

    #[test]
    fn remove_forgets_pointer_and_returns_presence() {
        let store = store();
        store
            .save(&pointer("https://show/1", "ep1", 100))
            .expect("save");
        assert!(store.remove("P", "https://show/1").expect("remove"));
        assert!(!store.remove("P", "https://show/1").expect("remove again"));
        assert_eq!(store.get("P", "https://show/1").expect("get"), None);
    }

    #[test]
    fn recent_orders_newest_first_and_respects_limit() {
        let store = store();
        store
            .save(&pointer("https://show/1", "ep1", 10))
            .expect("save 1");
        store
            .save(&pointer("https://show/2", "ep1", 30))
            .expect("save 2");
        store
            .save(&pointer("https://show/3", "ep1", 20))
            .expect("save 3");

        let recents = store.recent(2).expect("recent");
        assert_eq!(recents.len(), 2);
        assert_eq!(recents[0].detail_url, "https://show/2");
        assert_eq!(recents[1].detail_url, "https://show/3");
    }

    #[test]
    fn all_and_clear_work_properly() {
        let store = store();
        store
            .save(&pointer("https://show/1", "ep1", 10))
            .expect("save 1");
        store
            .save(&pointer("https://show/2", "ep1", 20))
            .expect("save 2");
        assert_eq!(store.all().expect("all").len(), 2);

        store.clear().expect("clear");
        assert!(store.all().expect("all after clear").is_empty());
    }
}
