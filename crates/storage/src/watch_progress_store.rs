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
}
