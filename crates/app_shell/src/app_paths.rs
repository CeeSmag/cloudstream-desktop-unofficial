use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// Folders and files the app owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    data_dir: PathBuf,
}

impl AppPaths {
    /// Stores everything under `data_dir` (used by tests and portable setups).
    pub fn for_directory(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Uses `CS_DATA_DIR`, else the `data` folder next to the program when it sits beside a
    /// `portable.txt` (the portable app), else `%LOCALAPPDATA%\\CloudStreamRs`, else
    /// `~/.local/share/cloudstream-rs`.
    pub fn from_environment() -> Result<Self> {
        let program_folder = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let data_dir = resolve_data_dir(DataDirChoices {
            explicit: std::env::var_os("CS_DATA_DIR").map(PathBuf::from),
            portable: program_folder.as_deref().and_then(portable_data_dir),
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            home: std::env::var_os("HOME").map(PathBuf::from),
        })
        .ok_or_else(|| anyhow!("cannot decide where to store data; set CS_DATA_DIR"))?;
        Ok(Self { data_dir })
    }

    /// Root folder of everything the app stores.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The SQLite database file.
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("cloudstream.db")
    }

    /// Folder that holds downloaded `.cs3` files.
    pub fn extensions_dir(&self) -> PathBuf {
        self.data_dir.join("extensions")
    }

    /// The log file.
    pub fn log_file(&self) -> PathBuf {
        self.data_dir.join("logs").join("cloudstream.log")
    }
}

/// Marks a folder as a portable install: everything the app stores goes next to the program.
pub const PORTABLE_MARKER: &str = "portable.txt";

/// Where each way of choosing the data folder points, when it applies.
#[derive(Debug, Default)]
struct DataDirChoices {
    explicit: Option<PathBuf>,
    portable: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
    home: Option<PathBuf>,
}

/// The `data` folder beside the program when the program folder holds the portable marker.
fn portable_data_dir(program_folder: &Path) -> Option<PathBuf> {
    program_folder
        .join(PORTABLE_MARKER)
        .is_file()
        .then(|| program_folder.join("data"))
}

fn resolve_data_dir(choices: DataDirChoices) -> Option<PathBuf> {
    choices
        .explicit
        .or(choices.portable)
        .or_else(|| {
            choices
                .local_app_data
                .map(|base| base.join("CloudStreamRs"))
        })
        .or_else(|| {
            choices
                .home
                .map(|base| base.join(".local").join("share").join("cloudstream-rs"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choices(
        explicit: Option<&str>,
        portable: Option<&str>,
        local: Option<&str>,
        home: Option<&str>,
    ) -> DataDirChoices {
        let path = |text: Option<&str>| text.map(PathBuf::from);
        DataDirChoices {
            explicit: path(explicit),
            portable: path(portable),
            local_app_data: path(local),
            home: path(home),
        }
    }

    #[test]
    fn explicit_directory_wins_over_everything() {
        let chosen = resolve_data_dir(choices(Some("x"), Some("p"), Some("local"), Some("home")));
        assert_eq!(chosen, Some(PathBuf::from("x")));
    }

    #[test]
    fn a_portable_install_beats_the_user_profile() {
        let chosen = resolve_data_dir(choices(None, Some("p"), Some("local"), Some("home")));
        assert_eq!(chosen, Some(PathBuf::from("p")));
    }

    #[test]
    fn local_app_data_is_preferred_over_home() {
        let chosen = resolve_data_dir(choices(None, None, Some("local"), Some("home")));
        assert_eq!(chosen, Some(PathBuf::from("local").join("CloudStreamRs")));
    }

    #[test]
    fn home_is_the_last_resort_and_nothing_means_none() {
        assert!(resolve_data_dir(choices(None, None, None, Some("home"))).is_some());
        assert_eq!(resolve_data_dir(DataDirChoices::default()), None);
    }

    #[test]
    fn only_a_folder_with_the_marker_is_portable() {
        let folder = std::env::temp_dir().join(format!("cs_portable_{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("folder");
        assert_eq!(portable_data_dir(&folder), None);
        std::fs::write(folder.join(PORTABLE_MARKER), b"").expect("marker");
        assert_eq!(portable_data_dir(&folder), Some(folder.join("data")));
        std::fs::remove_dir_all(&folder).expect("cleanup");
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
