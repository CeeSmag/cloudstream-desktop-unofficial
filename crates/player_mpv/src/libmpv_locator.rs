use std::path::PathBuf;

use player_api::PlayerError;

use crate::mpv_player::{MpvPlayer, PlayerOptions};

/// Environment variable that points at the libmpv shared library.
const LIBMPV_PATH_VARIABLE: &str = "CS_LIBMPV_PATH";

/// Library file names to look for, per platform.
const LIBRARY_NAMES: &[&str] = if cfg!(windows) {
    &["libmpv-2.dll", "mpv-2.dll"]
} else if cfg!(target_os = "macos") {
    &["libmpv.dylib"]
} else {
    &["libmpv.so.2", "libmpv.so"]
};

/// Where libmpv may be: the explicit path, then the folders to search, then (Linux/macOS) the bare
/// names the system loader resolves.
struct Sources {
    explicit: Option<PathBuf>,
    folders: Vec<PathBuf>,
    system_loader: bool,
}

impl Sources {
    fn from_environment() -> Self {
        let exe_folder = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from));
        Self {
            explicit: std::env::var_os(LIBMPV_PATH_VARIABLE).map(PathBuf::from),
            folders: [exe_folder, std::env::current_dir().ok()]
                .into_iter()
                .flatten()
                .collect(),
            system_loader: !cfg!(windows),
        }
    }

    /// Files that exist, best first, then the bare names for the system loader.
    fn candidates(&self) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = Vec::new();
        let existing = self.searched().into_iter().filter(|path| path.is_file());
        for path in existing {
            if !found.contains(&path) {
                found.push(path);
            }
        }
        if self.system_loader {
            found.extend(LIBRARY_NAMES.iter().map(PathBuf::from));
        }
        found
    }

    /// Every file looked for, whether or not it exists.
    fn searched(&self) -> Vec<PathBuf> {
        let named = self
            .folders
            .iter()
            .flat_map(|folder| LIBRARY_NAMES.iter().map(move |name| folder.join(name)));
        self.explicit.iter().cloned().chain(named).collect()
    }

    fn not_found(&self) -> PlayerError {
        let searched: Vec<String> = self
            .searched()
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        PlayerError::Unavailable(format!(
            "libmpv not found. Set {LIBMPV_PATH_VARIABLE} or place {} next to the app. Searched: {}",
            LIBRARY_NAMES.join(" / "),
            searched.join(", ")
        ))
    }
}

/// Finds all candidate paths where libmpv might live, ordered from highest to lowest
/// preference.
pub fn locate_libmpv_candidates() -> Vec<PathBuf> {
    Sources::from_environment().candidates()
}

/// Finds the libmpv shared library: `CS_LIBMPV_PATH`, then next to the executable, then the
/// working folder. On Linux/macOS the system loader path is tried as a last resort.
pub fn locate_libmpv() -> Result<PathBuf, PlayerError> {
    let sources = Sources::from_environment();
    sources
        .candidates()
        .into_iter()
        .next()
        .ok_or_else(|| sources.not_found())
}

/// Discovers candidate libmpv library paths and starts the player with the first working one.
#[tracing::instrument(level = "debug", skip_all, err)]
pub fn auto_detect_player(options: &PlayerOptions) -> Result<MpvPlayer, PlayerError> {
    let sources = Sources::from_environment();
    let candidates = sources.candidates();
    if candidates.is_empty() {
        return Err(sources.not_found());
    }
    let mut last_error = None;
    for path in &candidates {
        match MpvPlayer::new(path, options) {
            Ok(player) => {
                tracing::info!(candidate = %path.display(), "libmpv loaded successfully");
                return Ok(player);
            }
            Err(error) => {
                tracing::warn!(
                    candidate = %path.display(),
                    %error,
                    "candidate libmpv failed to load; trying next candidate"
                );
                last_error = Some(error);
            }
        }
    }
    match last_error {
        Some(error) => Err(error),
        None => Err(PlayerError::Unavailable(
            "all libmpv candidates failed to load".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(folders: Vec<PathBuf>, system_loader: bool) -> Sources {
        Sources {
            explicit: None,
            folders,
            system_loader,
        }
    }

    fn library_in(folder: &std::path::Path) -> PathBuf {
        let path = folder.join(LIBRARY_NAMES.first().copied().unwrap_or("libmpv"));
        std::fs::write(&path, b"x").expect("library stand-in written");
        path
    }

    #[test]
    fn explicit_path_comes_before_folders_and_system_names() {
        let (one, two) = (tempfile_dir("a"), tempfile_dir("b"));
        let (first, second) = (library_in(&one), library_in(&two));
        let mut all = sources(vec![one], true);
        all.explicit = Some(second.clone());
        let found = all.candidates();
        assert_eq!(found.first(), Some(&second));
        assert_eq!(found.get(1), Some(&first));
        assert_eq!(
            found.last(),
            LIBRARY_NAMES.last().map(PathBuf::from).as_ref()
        );
    }

    #[test]
    fn a_missing_explicit_path_is_skipped_but_reported() {
        let mut none = sources(Vec::new(), false);
        none.explicit = Some(PathBuf::from("/nowhere/libmpv"));
        assert!(none.candidates().is_empty());
        let message = none.not_found().to_string();
        assert!(message.contains("/nowhere/libmpv"), "{message}");
        assert!(message.contains(LIBMPV_PATH_VARIABLE), "{message}");
    }

    #[test]
    fn without_the_system_loader_only_real_files_are_candidates() {
        let folder = tempfile_dir("c");
        assert!(sources(vec![folder.clone()], false).candidates().is_empty());
        let file = library_in(&folder);
        assert_eq!(sources(vec![folder], false).candidates(), vec![file]);
    }

    fn tempfile_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mpv_locator_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp folder created");
        dir
    }
}
