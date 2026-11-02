use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;

/// Starts logging to the log file; falls back to stderr when the file cannot be opened.
pub fn start_logging(log_file: &Path) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let opened = log_file
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .and_then(|_| OpenOptions::new().create(true).append(true).open(log_file));
    match opened {
        Ok(file) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .init();
        }
        Err(error) => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
            tracing::warn!(%error, path = %log_file.display(), "cannot open log file, logging to stderr");
        }
    }
}
