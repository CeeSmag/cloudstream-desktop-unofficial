# storage

Settings, repositories, installed extensions, bookmarks and watch progress in one SQLite file.

- **Public API:** `Database` plus one store per concern (`SettingsStore`, `RepositoryStore`,
  `InstalledExtensionStore`, `BookmarkStore`, `WatchProgressStore`). Stores share an `Arc<Database>`.
- **Schema:** forward-only steps in `src/schema_migrations.rs`; add a new string to extend.
- **Time:** callers pass timestamps in; nothing here reads the clock.
- **Test:** `cargo test -p storage` (in-memory databases).
