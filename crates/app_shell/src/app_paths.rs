use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// Folders and files the app owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppPaths {
    data_dir: PathBuf,
}

impl AppPaths {
    /// Uses `CS_DATA_DIR`, else `%LOCALAPPDATA%\\CloudStreamRs`, else `~/.local/share/cloudstream-rs`.
    pub(crate) fn from_environment() -> Result<Self> {
        let data_dir = resolve_data_dir(
            std::env::var_os("CS_DATA_DIR").map(PathBuf::from),
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            std::env::var_os("HOME").map(PathBuf::from),
        )
        .ok_or_else(|| anyhow!("cannot decide where to store data; set CS_DATA_DIR"))?;
        Ok(Self { data_dir })
    }

    pub(crate) fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub(crate) fn database_file(&self) -> PathBuf {
        self.data_dir.join("cloudstream.db")
    }

    pub(crate) fn extensions_dir(&self) -> PathBuf {
        self.data_dir.join("extensions")
    }

    pub(crate) fn log_file(&self) -> PathBuf {
        self.data_dir.join("logs").join("cloudstream.log")
    }
}

fn resolve_data_dir(
    explicit: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    explicit
        .or_else(|| local_app_data.map(|base| base.join("CloudStreamRs")))
        .or_else(|| home.map(|base| base.join(".local").join("share").join("cloudstream-rs")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_directory_wins() {
        let chosen = resolve_data_dir(Some("x".into()), Some("local".into()), Some("home".into()));
        assert_eq!(chosen, Some(PathBuf::from("x")));
    }

    #[test]
    fn local_app_data_is_preferred_over_home() {
        let chosen = resolve_data_dir(None, Some("local".into()), Some("home".into()));
        assert_eq!(chosen, Some(PathBuf::from("local").join("CloudStreamRs")));
    }

    #[test]
    fn home_is_the_last_resort_and_nothing_means_none() {
        assert!(resolve_data_dir(None, None, Some("home".into())).is_some());
        assert_eq!(resolve_data_dir(None, None, None), None);
    }

    #[test]
    fn derived_paths_live_inside_the_data_directory() {
        let paths = AppPaths {
            data_dir: PathBuf::from("d"),
        };
        assert_eq!(
            paths.database_file(),
            PathBuf::from("d").join("cloudstream.db")
        );
        assert_eq!(
            paths.extensions_dir(),
            PathBuf::from("d").join("extensions")
        );
    }
}
