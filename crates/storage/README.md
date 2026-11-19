# storage

Settings, repositories, installed extensions, bookmarks, watch progress and resume
pointers in one SQLite file (`<data_dir>/cloudstream.db`, WAL).

- **Public API:** `Database` plus one store per concern (`SettingsStore`, `RepositoryStore`,
  `InstalledExtensionStore`, `BookmarkStore`, `WatchProgressStore`, `ResumeStore`). Stores share
  an `Arc<Database>`.
- **Schema:** forward-only steps in `src/schema_migrations.rs` (`PRAGMA user_version`); append a
  new string to extend, never edit or reorder earlier steps. Current versions:
  1. `settings`, `repositories`, `installed_extensions`, `bookmarks`, `watch_progress`
  2. `icon_url` on `repositories` and `installed_extensions` (NULL = not fetched, empty = none)
  3. `resume_pointers` (one row per show/movie) plus a one-shot copy of the latest
     `watch_progress` row per `(provider, detail_url)` (empty `detail_url` skipped; season/episode
     left NULL)
  4. `installed_at` on `installed_extensions` (`DEFAULT 0` for existing rows)
- **Time:** callers pass timestamps in; nothing here reads the clock.
- **Test:** `cargo test -p storage` (in-memory databases).
