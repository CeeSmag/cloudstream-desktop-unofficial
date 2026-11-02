use std::path::PathBuf;

use player_api::PlayerError;

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

/// Finds the libmpv shared library: `CS_LIBMPV_PATH`, then next to the executable, then the
/// working folder. On Linux/macOS the system loader path is tried as a last resort.
pub fn locate_libmpv() -> Result<PathBuf, PlayerError> {
    let mut searched = Vec::new();
    if let Some(explicit) = std::env::var_os(LIBMPV_PATH_VARIABLE).map(PathBuf::from) {
        if explicit.is_file() {
            return Ok(explicit);
        }
        searched.push(explicit);
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
            if candidate.is_file() {
                return Ok(candidate);
            }
            searched.push(candidate);
        }
    }
    if cfg!(windows) {
        return Err(PlayerError::Unavailable(format!(
            "libmpv not found. Set {LIBMPV_PATH_VARIABLE} or place {} next to the app. Searched: {}",
            LIBRARY_NAMES.join(" / "),
            searched.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        )));
    }
    Ok(PathBuf::from(
        LIBRARY_NAMES.first().copied().unwrap_or("libmpv.so"),
    ))
}
