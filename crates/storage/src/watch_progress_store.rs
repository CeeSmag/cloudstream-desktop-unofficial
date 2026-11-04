use std::sync::Arc;

use core_model::WatchProgress;
use rusqlite::{params, OptionalExtension, Row};

use crate::{Database, StorageError};

/// Playback positions and "continue watching" history.
#[derive(Clone)]
pub struct WatchProgressStore {
    database: Arc<Database>,
}

fn read_progress(row: &Row<'_>) -> rusqlite::Result<WatchProgress> {
    Ok(WatchProgress {
        provider: row.get(0)?,
        media_key: row.get(1)?,
        title: row.get(2)?,
        detail_url: row.get(3)?,
        poster_url: row.get(4)?,
        position_ms: row.get(5)?,
        duration_ms: row.get(6)?,
        updated_at_ms: row.get(7)?,
        season: None,
        episode: None,
    })
}

const SELECT_COLUMNS: &str =
    "provider, media_key, title, detail_url, poster_url, position_ms, duration_ms, updated_at";

impl WatchProgressStore {
    /// Creates a store over a shared database.
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    /// Saves progress, replacing any earlier position for the same item.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn save(&self, progress: &WatchProgress) -> Result<(), StorageError> {
        self.database.with("save watch progress", |connection| {
            connection
                .execute(
                    "INSERT OR REPLACE INTO watch_progress
                         (provider, media_key, title, detail_url, poster_url,
                          position_ms, duration_ms, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        progress.provider,
                        progress.media_key,
                        progress.title,
                        progress.detail_url,
                        progress.poster_url,
                        progress.position_ms,
                        progress.duration_ms,
                        progress.updated_at_ms
                    ],
                )
                .map(drop)
        })
    }

    /// Reads the saved progress of one item.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, media_key = ?media_key), err)]
    pub fn get(
        &self,
        provider: &str,
        media_key: &str,
    ) -> Result<Option<WatchProgress>, StorageError> {
        self.database.with("read watch progress", |connection| {
            connection
                .query_row(
                    &format!(
                        "SELECT {SELECT_COLUMNS} FROM watch_progress WHERE provider = ?1 AND media_key = ?2"
                    ),
                    params![provider, media_key],
                    read_progress,
                )
                .optional()
        })
    }

    /// Forgets the saved progress of one item. Returns whether there was any.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, media_key = ?media_key), err)]
    pub fn remove(&self, provider: &str, media_key: &str) -> Result<bool, StorageError> {
        self.database.with("remove watch progress", |connection| {
            connection
                .execute(
                    "DELETE FROM watch_progress WHERE provider = ?1 AND media_key = ?2",
                    params![provider, media_key],
                )
                .map(|rows| rows > 0)
        })
    }

    /// Lists the most recently watched items, newest first.
    #[tracing::instrument(level = "trace", skip_all, fields(limit = ?limit), err)]
    pub fn recent(&self, limit: u32) -> Result<Vec<WatchProgress>, StorageError> {
        self.database.with("list recent progress", |connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM watch_progress ORDER BY updated_at DESC LIMIT ?1"
            ))?;
            let rows = statement.query_map(params![limit], read_progress)?;
            rows.collect()
        })
    }

    /// Finds all episode media keys for a given show.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
    pub fn episodes_for_show(
        &self,
        provider: &str,
        detail_url: &str,
    ) -> Result<Vec<String>, StorageError> {
        self.database.with("list show episodes", |connection| {
            let mut statement = connection.prepare(
                "SELECT media_key FROM watch_progress WHERE provider = ?1 AND detail_url = ?2",
            )?;
            let rows = statement.query_map(params![provider, detail_url], |row| row.get(0))?;
            rows.collect()
        })
    }

    /// Deletes all watch progress entries for a given show. Returns number of rows deleted.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
    pub fn remove_for_show(&self, provider: &str, detail_url: &str) -> Result<usize, StorageError> {
        self.database.with("remove show progress", |connection| {
            connection.execute(
                "DELETE FROM watch_progress WHERE provider = ?1 AND detail_url = ?2",
                params![provider, detail_url],
            )
        })
    }

    /// Lists all (provider, media_key) pairs currently stored.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn all_media_keys(&self) -> Result<Vec<(String, String)>, StorageError> {
        self.database.with("list all media keys", |connection| {
            let mut statement =
                connection.prepare("SELECT provider, media_key FROM watch_progress")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect()
        })
    }

    /// Clears all watch progress records.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub fn clear(&self) -> Result<(), StorageError> {
        self.database
            .with("clear all watch progress", |connection| {
                connection
                    .execute("DELETE FROM watch_progress", [])
                    .map(drop)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(key: &str, position_ms: i64, updated_at_ms: i64) -> WatchProgress {
        WatchProgress {
            provider: "P".to_owned(),
            media_key: key.to_owned(),
            title: format!("title {key}"),
            position_ms,
            duration_ms: 100_000,
            updated_at_ms,
            ..WatchProgress::default()
        }
    }

    fn store() -> WatchProgressStore {
        WatchProgressStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
    }

    #[test]
    fn save_replaces_the_position_for_the_same_item() {
        let store = store();
        store.save(&progress("a", 10, 1)).expect("save");
        store.save(&progress("a", 55, 2)).expect("save again");
        assert_eq!(
            store
                .get("P", "a")
                .expect("get")
                .expect("present")
                .position_ms,
            55
        );
    }

    #[test]
    fn remove_forgets_only_that_item_and_says_whether_it_was_there() {
        let store = store();
        store.save(&progress("a", 10, 1)).expect("save");
        store.save(&progress("b", 10, 2)).expect("save");
        assert!(store.remove("P", "a").expect("remove"));
        assert!(!store.remove("P", "a").expect("remove again"));
        assert!(!store.remove("Other", "b").expect("other provider"));
        assert_eq!(store.get("P", "a").expect("get"), None);
        assert!(store.get("P", "b").expect("get").is_some());
    }

    #[test]
    fn get_returns_none_for_unwatched_item() {
        assert_eq!(store().get("P", "nope").expect("get"), None);
    }

    #[test]
    fn recent_orders_newest_first_and_honours_the_limit() {
        let store = store();
        for (key, at) in [("a", 1), ("b", 3), ("c", 2)] {
            store.save(&progress(key, 1, at)).expect("save");
        }
        let keys: Vec<_> = store
            .recent(2)
            .expect("recent")
            .into_iter()
            .map(|p| p.media_key)
            .collect();
        assert_eq!(keys, ["b", "c"]);
    }

    #[test]
    fn episodes_for_show_and_remove_for_show_work_together() {
        let store = store();
        let mut p1 = progress("ep1", 10, 1);
        p1.detail_url = "https://show/1".to_owned();
        let mut p2 = progress("ep2", 20, 2);
        p2.detail_url = "https://show/1".to_owned();
        let mut p3 = progress("other", 30, 3);
        p3.detail_url = "https://show/2".to_owned();
        store.save(&p1).expect("save 1");
        store.save(&p2).expect("save 2");
        store.save(&p3).expect("save 3");

        let eps = store
            .episodes_for_show("P", "https://show/1")
            .expect("episodes");
        assert_eq!(eps.len(), 2);
        assert!(eps.contains(&"ep1".to_owned()));
        assert!(eps.contains(&"ep2".to_owned()));

        let deleted = store
            .remove_for_show("P", "https://show/1")
            .expect("remove show");
        assert_eq!(deleted, 2);
        let remaining = store
            .episodes_for_show("P", "https://show/1")
            .expect("empty");
        assert!(remaining.is_empty());
        assert!(store.get("P", "other").expect("get").is_some());
    }

    #[test]
    fn all_media_keys_and_clear_work_correctly() {
        let store = store();
        store.save(&progress("k1", 10, 1)).expect("save");
        store.save(&progress("k2", 20, 2)).expect("save");
        let keys = store.all_media_keys().expect("keys");
        assert_eq!(keys.len(), 2);

        store.clear().expect("clear");
        let after = store.all_media_keys().expect("keys empty");
        assert!(after.is_empty());
    }
}
