//! Swapping in the downloaded file. The running program cannot replace itself, so a small helper
//! script waits for the app to exit, moves the new file over the old one and starts it.

use std::path::{Path, PathBuf};

use crate::update_error::UpdateError;

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

/// The file the update replaces: the portable exe or the AppImage the app was started from, else the
/// running program. Fails when its folder cannot be written to.
pub(crate) fn target_executable_path() -> Result<PathBuf, UpdateError> {
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

/// The Windows helper: waits for process `pid` to end, moves the file into place, starts it.
#[cfg(any(windows, test))]
fn windows_script(pid: u32, downloaded: &Path, target: &Path) -> String {
    format!(
        "@echo off\r\n\
         :wait\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         tasklist /fi \"PID eq {pid}\" | find \"{pid}\" >nul\r\n\
         if not errorlevel 1 goto wait\r\n\
         move /y \"{downloaded}\" \"{target}\"\r\n\
         start \"\" \"{target}\"\r\n\
         del \"%~f0\"\r\n",
        downloaded = downloaded.display(),
        target = target.display()
    )
}

/// The Linux/macOS helper: the same steps as `windows_script` in `sh`.
#[cfg(any(not(windows), test))]
fn unix_script(pid: u32, downloaded: &Path, target: &Path) -> String {
    format!(
        "#!/bin/sh\n\
         while kill -0 {pid} 2>/dev/null; do sleep 0.5; done\n\
         sleep 0.5\n\
         chmod +x \"{downloaded}\" 2>/dev/null || true\n\
         mv -f \"{downloaded}\" \"{target}\" || cp -f \"{downloaded}\" \"{target}\"\n\
         chmod +x \"{target}\" 2>/dev/null || true\n\
         unset APPDIR\n\
         nohup \"{target}\" >/dev/null 2>&1 &\n\
         rm -f \"$0\"\n",
        downloaded = downloaded.display(),
        target = target.display()
    )
}

/// Writes the helper script into `folder` (the update attempt's own folder) and starts it detached.
/// The caller exits the app right after.
#[cfg(windows)]
pub(crate) fn start_install_helper(
    downloaded: &Path,
    target: &Path,
    folder: &Path,
) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    /// Windows process flag: run the console helper without showing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let pid = std::process::id();
    let script = folder.join("install.bat");
    std::fs::write(&script, windows_script(pid, downloaded, target))
        .map_err(|error| UpdateError::disk("write", &script, error))?;
    std::process::Command::new("cmd.exe")
        .arg("/c")
        .arg(&script)
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
    target: &Path,
    folder: &Path,
) -> Result<(), UpdateError> {
    let pid = std::process::id();
    let script = folder.join("install.sh");
    std::fs::write(&script, unix_script(pid, downloaded, target))
        .map_err(|error| UpdateError::disk("write", &script, error))?;
    // No permission change needed: the script is run as `sh <script>`, not executed directly.
    std::process::Command::new("sh")
        .arg(&script)
        .spawn()
        .map_err(UpdateError::Spawn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_script_for_pid_waits_then_moves_then_starts() {
        let script = windows_script(42, Path::new(r"C:\t\new.exe"), Path::new(r"C:\app\cs.exe"));
        let wait = script.find("PID eq 42").unwrap();
        let swap = script
            .find(r#"move /y "C:\t\new.exe" "C:\app\cs.exe""#)
            .unwrap();
        let start = script.find(r#"start "" "C:\app\cs.exe""#).unwrap();
        assert!(wait < swap && swap < start);
    }

    #[test]
    fn unix_script_for_pid_waits_then_moves_then_starts() {
        let script = unix_script(42, Path::new("/tmp/new"), Path::new("/opt/cs.AppImage"));
        let wait = script.find("kill -0 42").unwrap();
        let swap = script
            .find(r#"mv -f "/tmp/new" "/opt/cs.AppImage""#)
            .unwrap();
        let start = script.find(r#"nohup "/opt/cs.AppImage""#).unwrap();
        assert!(wait < swap && swap < start);
    }
}
