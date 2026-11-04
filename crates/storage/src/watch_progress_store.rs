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

    /// Every saved position of one show's episodes, oldest first.
    #[tracing::instrument(level = "trace", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
    pub fn for_show(
        &self,
        provider: &str,
        detail_url: &str,
    ) -> Result<Vec<WatchProgress>, StorageError> {
        self.database.with("list show progress", |connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM watch_progress
                 WHERE provider = ?1 AND detail_url = ?2 ORDER BY updated_at"
            ))?;
            let rows = statement.query_map(params![provider, detail_url], read_progress)?;
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
#[path = "watch_progress_store_tests.rs"]
mod tests;
