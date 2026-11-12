use rusqlite::Connection;

/// Ordered schema steps; index + 1 is the schema version after the step ran.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
     CREATE TABLE repositories (url TEXT PRIMARY KEY, name TEXT NOT NULL, added_at INTEGER NOT NULL);
     CREATE TABLE installed_extensions (
         file_name TEXT PRIMARY KEY, internal_name TEXT NOT NULL, repository_url TEXT,
         version INTEGER NOT NULL, trusted INTEGER NOT NULL);
     CREATE TABLE bookmarks (
         provider TEXT NOT NULL, url TEXT NOT NULL, name TEXT NOT NULL, poster_url TEXT,
         media_type TEXT, added_at INTEGER NOT NULL, PRIMARY KEY (provider, url));
     CREATE TABLE watch_progress (
         provider TEXT NOT NULL, media_key TEXT NOT NULL, title TEXT NOT NULL,
         detail_url TEXT NOT NULL, poster_url TEXT, position_ms INTEGER NOT NULL,
         duration_ms INTEGER NOT NULL, updated_at INTEGER NOT NULL,
         PRIMARY KEY (provider, media_key));",
    // Icons: NULL means not looked at yet, an empty text means there is none.
    "ALTER TABLE repositories ADD COLUMN icon_url TEXT;
     ALTER TABLE installed_extensions ADD COLUMN icon_url TEXT;",
    // Resume pointers: one pointer per show/movie, pointing to the next episode.
    "CREATE TABLE resume_pointers (
         provider TEXT NOT NULL,
         detail_url TEXT NOT NULL,
         media_key TEXT NOT NULL,
         title TEXT NOT NULL,
         poster_url TEXT,
         season INTEGER,
         episode INTEGER,
         updated_at INTEGER NOT NULL,
         PRIMARY KEY (provider, detail_url)
     );
     INSERT OR REPLACE INTO resume_pointers
         (provider, detail_url, media_key, title, poster_url, season, episode, updated_at)
     SELECT w.provider, w.detail_url, w.media_key, w.title, w.poster_url, NULL, NULL, w.updated_at
     FROM watch_progress w
     INNER JOIN (
         SELECT provider, detail_url, MAX(updated_at) AS max_updated
         FROM watch_progress
         WHERE detail_url != ''
         GROUP BY provider, detail_url
     ) latest ON w.provider = latest.provider
             AND w.detail_url = latest.detail_url
             AND w.updated_at = latest.max_updated;",
];

/// SQLite stores the schema version as a signed integer; one that does not fit is reported as
/// an error instead of being guessed, because guessing could run the migrations again.
fn version_conversion_error(error: std::num::TryFromIntError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Integer, Box::new(error))
}

/// Brings the database schema up to date, running only the steps it has not seen.
pub(crate) fn migrate(connection: &Connection) -> rusqlite::Result<()> {
    let stored: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let current = usize::try_from(stored).map_err(version_conversion_error)?;
    for (index, step) in MIGRATIONS.iter().enumerate().skip(current) {
        let version = i64::try_from(index + 1).map_err(version_conversion_error)?;
        tracing::info!(version, "applying storage migration");
        connection.execute_batch(step)?;
        connection.pragma_update(None, "user_version", version)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_twice_is_a_no_op_the_second_time() {
        let connection = Connection::open_in_memory().expect("memory db");
        migrate(&connection).expect("first run");
        migrate(&connection).expect("second run");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, i64::try_from(MIGRATIONS.len()).expect("fits"));
    }

    #[test]
    fn migrate_a_negative_stored_version_is_an_error_and_changes_nothing() {
        let connection = Connection::open_in_memory().expect("memory db");
        connection
            .pragma_update(None, "user_version", -1)
            .expect("set version");
        assert!(migrate(&connection).is_err());
        let tables: i64 = connection
            .query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0))
            .expect("count");
        assert_eq!(tables, 0);
    }

    type MigratedPointer = (String, String, String, String, Option<String>, i64);

    fn seed_v2_database(connection: &Connection) {
        connection.execute_batch(MIGRATIONS[0]).expect("step 0");
        connection.execute_batch(MIGRATIONS[1]).expect("step 1");
        connection
            .pragma_update(None, "user_version", 2)
            .expect("version 2");
        connection
            .execute(
                "INSERT INTO watch_progress VALUES
                 ('P', 'ep1', 'Show 1', 'https://show/1', 'p1.jpg', 10, 100, 1000),
                 ('P', 'ep2', 'Show 1', 'https://show/1', 'p1.jpg', 20, 100, 2000),
                 ('P', 'movie1', 'Movie 1', 'https://movie/1', 'm1.jpg', 50, 100, 1500),
                 ('P', 'orphan', 'No Show', '', NULL, 50, 100, 3000)",
                [],
            )
            .expect("insert progress");
    }

    fn read_migrated_pointers(connection: &Connection) -> Vec<MigratedPointer> {
        let mut stmt = connection
            .prepare(
                "SELECT provider, detail_url, media_key, title, poster_url, updated_at
                 FROM resume_pointers ORDER BY detail_url",
            )
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .expect("query");
        rows.map(|r| r.expect("row")).collect()
    }

    #[test]
    fn migration_3_creates_resume_pointers_from_existing_progress() {
        let connection = Connection::open_in_memory().expect("memory db");
        seed_v2_database(&connection);
        migrate(&connection).expect("migrate to 3");
        let results = read_migrated_pointers(&connection);

        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0],
            (
                "P".to_owned(),
                "https://movie/1".to_owned(),
                "movie1".to_owned(),
                "Movie 1".to_owned(),
                Some("m1.jpg".to_owned()),
                1500
            )
        );
        assert_eq!(
            results[1],
            (
                "P".to_owned(),
                "https://show/1".to_owned(),
                "ep2".to_owned(),
                "Show 1".to_owned(),
                Some("p1.jpg".to_owned()),
                2000
            )
        );
    }
}
