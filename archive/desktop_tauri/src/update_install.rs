//! Putting a checked update in place. The running program cannot replace itself, so a helper script
//! (`update_helper_script.rs`) waits for the app to exit and then either moves the new file over the
//! old one (portable exe, AppImage) or runs the downloaded installer (installed Windows app).

use std::path::{Path, PathBuf};

use crate::update_error::UpdateError;

/// How the downloaded file replaces the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstallMethod {
    /// Move the download over this file (portable exe, AppImage, or the running program elsewhere).
    ReplaceFile(PathBuf),
    /// Run the downloaded Windows installer, which replaces the installed app and starts it. Moving
    /// the installer over the app's exe would leave the installer running under the app's name,
    /// trying to replace itself.
    RunInstaller,
}

/// True for the portable exe (`CS_PORTABLE_EXE`), an AppImage (`APPIMAGE`) or a copy with
/// `portable.txt` beside it; these update to the portable file instead of the installer.
pub(crate) fn is_portable_environment() -> bool {
    std::env::var_os("CS_PORTABLE_EXE").is_some()
        || std::env::var_os("APPIMAGE").is_some()
        || std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent()
                    .map(|folder| folder.join("portable.txt").exists())
            })
            .unwrap_or(false)
}

fn check_target_writable(target: &Path) -> Result<(), UpdateError> {
    let Some(folder) = target.parent() else {
        return Ok(());
    };
    let probe = folder.join(format!(".cs_update_test_{}", std::process::id()));
    if std::fs::write(&probe, b"").is_err() {
        return Err(UpdateError::ReadOnlyTarget(folder.to_path_buf()));
    }
    if let Err(error) = std::fs::remove_file(&probe) {
        // An empty file left beside the program does no harm; the update can go on.
        tracing::warn!(path = %probe.display(), %error, "cannot remove the write-test file");
    }
    Ok(())
}

fn existing_path_from_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.exists())
}

/// The file a portable update replaces: the portable exe or the AppImage the app was started from,
/// else the running program. Fails when its folder cannot be written to.
fn target_executable_path() -> Result<PathBuf, UpdateError> {
    let wrapper = existing_path_from_env("CS_PORTABLE_EXE").or_else(|| {
        cfg!(target_os = "linux")
            .then(|| existing_path_from_env("APPIMAGE"))
            .flatten()
    });
    let target = match wrapper {
        Some(path) => path,
        None => std::env::current_exe().map_err(UpdateError::RunningProgramUnknown)?,
    };
    check_target_writable(&target)?;
    Ok(target)
}

/// The method for a system: the installed Windows app runs the installer (and never needs its own
/// folder to be writable: the installer handles that); everything else replaces `target()`.
fn choose_install_method(
    is_windows: bool,
    is_portable: bool,
    target: impl FnOnce() -> Result<PathBuf, UpdateError>,
) -> Result<InstallMethod, UpdateError> {
    if is_windows && !is_portable {
        return Ok(InstallMethod::RunInstaller);
    }
    target().map(InstallMethod::ReplaceFile)
}

/// How this copy of the app updates. Matches the file `update_assets::select_platform_asset` picks:
/// the installer for an installed Windows app, the portable file otherwise.
pub(crate) fn install_method() -> Result<InstallMethod, UpdateError> {
    choose_install_method(
        cfg!(windows),
        is_portable_environment(),
        target_executable_path,
    )
}

/// Writes the helper script into `folder` (the update attempt's own folder) and starts it detached.
/// The caller exits the app right after.
#[cfg(windows)]
pub(crate) fn start_install_helper(
    downloaded: &Path,
    method: &InstallMethod,
    folder: &Path,
) -> Result<(), UpdateError> {
    use crate::update_helper_script::{
        bat_command, windows_installer_script, windows_replace_script, BAT_SCRIPT_NAME,
    };
    use std::os::windows::process::CommandExt;
    /// Windows process flag: run the console helper without showing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let pid = std::process::id();
    let text = match method {
        InstallMethod::ReplaceFile(target) => windows_replace_script(pid, downloaded, target)?,
        InstallMethod::RunInstaller => windows_installer_script(pid, downloaded)?,
    };
    let script = folder.join(BAT_SCRIPT_NAME);
    std::fs::write(&script, text).map_err(|error| UpdateError::disk("write", &script, error))?;
    bat_command(folder)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(UpdateError::Spawn)?;
    Ok(())
}

/// Writes the helper script into `folder` (the update attempt's own folder) and starts it detached.
/// The caller exits the app right after.
#[cfg(not(windows))]
pub(crate) fn start_install_helper(
    downloaded: &Path,
    method: &InstallMethod,
    folder: &Path,
) -> Result<(), UpdateError> {
    use crate::update_helper_script::{sh_command, unix_replace_script, SH_SCRIPT_NAME};

    // An installer is chosen only on Windows; here there is no file this system can install.
    let InstallMethod::ReplaceFile(target) = method else {
        return Err(UpdateError::NoAsset);
    };
    let text = unix_replace_script(std::process::id(), downloaded, target)?;
    let script = folder.join(SH_SCRIPT_NAME);
    std::fs::write(&script, text).map_err(|error| UpdateError::disk("write", &script, error))?;
    // No permission change needed: the script is run as `sh <script>`, not executed directly.
    sh_command(folder).spawn().map_err(UpdateError::Spawn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_exe() -> Result<PathBuf, UpdateError> {
        Ok(PathBuf::from(
            r"C:\Users\x\AppData\Local\CloudStream\CloudStream.exe",
        ))
    }

    fn unwritable() -> Result<PathBuf, UpdateError> {
        Err(UpdateError::ReadOnlyTarget(PathBuf::from(
            r"C:\Program Files",
        )))
    }

    #[test]
    fn choose_install_method_installed_windows_app_runs_the_installer() {
        let method = choose_install_method(true, false, app_exe).unwrap();
        assert_eq!(method, InstallMethod::RunInstaller);
    }

    #[test]
    fn choose_install_method_installed_windows_app_does_not_need_a_writable_folder() {
        let method = choose_install_method(true, false, unwritable).unwrap();
        assert_eq!(method, InstallMethod::RunInstaller);
    }

    #[test]
    fn choose_install_method_portable_windows_app_replaces_its_exe() {
        let method = choose_install_method(true, true, app_exe).unwrap();
        assert_eq!(method, InstallMethod::ReplaceFile(app_exe().unwrap()));
    }

    #[test]
    fn choose_install_method_linux_replaces_the_file_and_reports_a_read_only_folder() {
        let method = choose_install_method(false, true, app_exe).unwrap();
        assert_eq!(method, InstallMethod::ReplaceFile(app_exe().unwrap()));
        let result = choose_install_method(false, false, unwritable);
        assert!(matches!(result, Err(UpdateError::ReadOnlyTarget(_))));
    }
}
