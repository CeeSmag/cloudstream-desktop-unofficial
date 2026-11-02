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
];

/// Brings the database schema up to date, running only the steps it has not seen.
pub(crate) fn migrate(connection: &Connection) -> rusqlite::Result<()> {
    let current: usize = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    for (index, step) in MIGRATIONS.iter().enumerate().skip(current) {
        tracing::info!(version = index + 1, "applying storage migration");
        connection.execute_batch(step)?;
        connection.pragma_update(None, "user_version", index + 1)?;
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
        let version: usize = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, MIGRATIONS.len());
    }
}
