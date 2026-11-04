//! Settings, library, history and extension bookkeeping persisted in SQLite.
#![forbid(unsafe_code)]

mod bookmark_store;
mod database;
mod installed_extension_store;
mod repository_store;
mod resume_store;
mod schema_migrations;
mod settings_store;
mod storage_error;
mod watch_progress_store;

pub use bookmark_store::BookmarkStore;
pub use database::Database;
pub use installed_extension_store::InstalledExtensionStore;
pub use repository_store::RepositoryStore;
pub use resume_store::ResumeStore;
pub use settings_store::SettingsStore;
pub use storage_error::StorageError;
pub use watch_progress_store::WatchProgressStore;
