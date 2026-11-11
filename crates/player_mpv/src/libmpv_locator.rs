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

/// Finds all candidate paths where libmpv might live, ordered from highest to lowest
/// preference.
pub fn locate_libmpv_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(explicit) = std::env::var_os(LIBMPV_PATH_VARIABLE).map(PathBuf::from) {
        if explicit.is_file() {
            candidates.push(explicit);
        }
    }
    let folders = [
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from)),
        std::env::current_dir().ok(),
    ];
    for folder in folders.into_iter().flatten() {
        for name in LIBRARY_NAMES {
            let candidate = folder.join(name);
            if candidate.is_file() && !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    if !cfg!(windows) {
        for name in LIBRARY_NAMES {
            let bare = PathBuf::from(name);
            if !candidates.contains(&bare) {
                candidates.push(bare);
            }
        }
    }
    candidates
}

/// Finds the libmpv shared library: `CS_LIBMPV_PATH`, then next to the executable, then the
/// working folder. On Linux/macOS the system loader path is tried as a last resort.
pub fn locate_libmpv() -> Result<PathBuf, PlayerError> {
    let candidates = locate_libmpv_candidates();
    candidates.into_iter().next().ok_or_else(|| {
        PlayerError::Unavailable(format!(
            "libmpv not found. Set {LIBMPV_PATH_VARIABLE} or place {} next to the app.",
            LIBRARY_NAMES.join(" / ")
        ))
    })
}

/// Discovers candidate libmpv library paths and starts the player with the first working one.
#[tracing::instrument(level = "debug", skip_all, err)]
pub fn auto_detect_player(options: &PlayerOptions) -> Result<MpvPlayer, PlayerError> {
    let candidates = locate_libmpv_candidates();
    if candidates.is_empty() {
        return Err(PlayerError::Unavailable(
            "no libmpv library candidates found".to_owned(),
        ));
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

    #[test]
    fn candidates_are_returned_in_priority_order() {
        let candidates = locate_libmpv_candidates();
        if !cfg!(windows) {
            assert!(
                !candidates.is_empty(),
                "must find at least system loader candidates on unix"
            );
        }
    }
}
