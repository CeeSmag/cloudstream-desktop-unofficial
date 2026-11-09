//! The helper behind "Delete all saved data": a separate process that outlives the app. It waits for
//! the app to close, ends what still holds files in the data folder (the sidecar's Java), deletes
//! everything, and starts the app again. The app never deletes anything while it is running, so no
//! open file can get in the way.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What the helper does on Windows. The folder the running program was unpacked into (`$KeepDir`, a
/// `runtime-*` folder beside the data) stays, because the app starts from it again; all older ones go.
#[cfg(any(windows, test))]
const WINDOWS_SCRIPT: &str = r#"param([int]$AppPid, [string]$DataDir, [string]$Relaunch = '', [string]$KeepDir = '')
$ErrorActionPreference = 'SilentlyContinue'
# 1. Wait for the app to close; after 30 seconds end it.
$app = Get-Process -Id $AppPid
if ($app -and -not $app.WaitForExit(30000)) { $app.Kill() }
# 2. End what still holds files in the data folder: the sidecar's Java.
Get-CimInstance Win32_Process -Filter "Name = 'java.exe'" |
    Where-Object { $_.CommandLine -and $_.CommandLine.Contains($DataDir) } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
Start-Sleep -Milliseconds 500
# 3. Delete everything except the folder of the running program, trying again while something is in use.
for ($try = 0; $try -lt 40; $try++) {
    $left = 0
    Get-ChildItem -LiteralPath $DataDir -Force | Where-Object { $_.Name -ne $KeepDir } | ForEach-Object {
        Remove-Item -LiteralPath $_.FullName -Recurse -Force
        if (Test-Path -LiteralPath $_.FullName) { $left++ }
    }
    if ($left -eq 0) { break }
    Start-Sleep -Milliseconds 500
}
# 4. Start the app again, as a new process, then remove this script.
if ($Relaunch) { Start-Process -FilePath $Relaunch }
Remove-Item -LiteralPath $PSCommandPath -Force
"#;

/// What the helper does on Linux and macOS (same steps as the Windows script).
#[cfg(any(unix, test))]
const UNIX_SCRIPT: &str = r#"#!/bin/sh
APP_PID="$1"; DATA_DIR="$2"; RELAUNCH="$3"; KEEP="$4"
n=0
while kill -0 "$APP_PID" 2>/dev/null; do
  n=$((n+1))
  if [ "$n" -gt 60 ]; then kill -9 "$APP_PID" 2>/dev/null; break; fi
  sleep 0.5
done
pkill -9 -f "cloudstream.data.dir=$DATA_DIR" 2>/dev/null
sleep 0.5
n=0
while [ "$n" -lt 40 ]; do
  find "$DATA_DIR" -mindepth 1 -maxdepth 1 ! -name "$KEEP" -exec rm -rf {} + 2>/dev/null
  if [ -z "$(find "$DATA_DIR" -mindepth 1 -maxdepth 1 ! -name "$KEEP" 2>/dev/null | head -n 1)" ]; then break; fi
  n=$((n+1)); sleep 0.5
done
if [ -n "$RELAUNCH" ]; then nohup "$RELAUNCH" >/dev/null 2>&1 & fi
rm -f "$0"
"#;

/// What starts the app again: the portable exe or the AppImage when that is what is running (the
/// program inside them is only a part of them), else the running program itself.
pub fn relaunch_target(
    portable: Option<PathBuf>,
    appimage: Option<PathBuf>,
    current: Option<PathBuf>,
) -> Option<PathBuf> {
    portable.or(appimage).or(current)
}

/// Starts the helper for `data_dir`. It waits for this process to end, so the caller must exit soon.
pub fn start_helper(data_dir: &Path) -> io::Result<()> {
    let relaunch = relaunch_target(
        std::env::var_os("CS_PORTABLE_EXE").map(PathBuf::from),
        std::env::var_os("APPIMAGE").map(PathBuf::from),
        std::env::current_exe().ok(),
    );
    let exe = std::env::current_exe().ok();
    let keep = app_shell::running_folder_name(data_dir, exe.as_deref());
    tracing::info!(keep = ?keep, relaunch = ?relaunch, "starting the helper that deletes all saved data; it keeps only the running program's folder");
    let script = write_script()?;
    let mut command = helper_command(&script, data_dir, (relaunch.as_deref(), keep.as_deref()));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    command.spawn().map(|_| ())
}

#[cfg(windows)]
fn write_script() -> io::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("cs_delete_all_data_{}.ps1", std::process::id()));
    std::fs::write(&path, WINDOWS_SCRIPT)?;
    Ok(path)
}

#[cfg(not(windows))]
fn write_script() -> io::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("cs_delete_all_data_{}.sh", std::process::id()));
    std::fs::write(&path, UNIX_SCRIPT)?;
    Ok(path)
}

#[cfg(windows)]
fn helper_command(
    script: &Path,
    data_dir: &Path,
    targets: (Option<&Path>, Option<&str>),
) -> Command {
    let (relaunch, keep) = targets;
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .args(["-WindowStyle", "Hidden", "-File"])
        .arg(script)
        .args(["-AppPid", &std::process::id().to_string(), "-DataDir"])
        .arg(data_dir);
    if let Some(program) = relaunch {
        command.arg("-Relaunch").arg(program);
    }
    if let Some(folder) = keep {
        command.arg("-KeepDir").arg(folder);
    }
    command
}

#[cfg(not(windows))]
fn helper_command(
    script: &Path,
    data_dir: &Path,
    targets: (Option<&Path>, Option<&str>),
) -> Command {
    let (relaunch, keep) = targets;
    let mut command = Command::new("sh");
    command
        .arg(script)
        .arg(std::process::id().to_string())
        .arg(data_dir)
        .arg(relaunch.map(Path::as_os_str).unwrap_or_default())
        .arg(keep.unwrap_or_default());
    command
}

/// Lets the helper keep running after this process is gone. It gets its own hidden console: a
/// console program started with no console at all (`DETACHED_PROCESS`) can fail to start.
#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> Option<PathBuf> {
        Some(PathBuf::from(text))
    }

    #[test]
    fn the_portable_exe_is_started_again_before_anything_else() {
        let chosen = relaunch_target(
            path("portable.exe"),
            path("app.AppImage"),
            path("inner.exe"),
        );
        assert_eq!(chosen, path("portable.exe"));
    }

    #[test]
    fn an_appimage_is_started_again_by_its_own_file() {
        assert_eq!(
            relaunch_target(None, path("app.AppImage"), path("inner")),
            path("app.AppImage")
        );
    }

    #[test]
    fn a_plain_install_starts_its_own_program() {
        assert_eq!(
            relaunch_target(None, None, path("cloudstream.exe")),
            path("cloudstream.exe")
        );
        assert_eq!(relaunch_target(None, None, None), None);
    }

    #[test]
    fn both_scripts_keep_only_the_running_program_folder_and_wait_for_the_app() {
        assert!(WINDOWS_SCRIPT.contains("$_.Name -ne $KeepDir"));
        assert!(
            !WINDOWS_SCRIPT.contains("runtime-*"),
            "older program folders must be deleted too"
        );
        assert!(UNIX_SCRIPT.contains("! -name \"$KEEP\""));
        assert!(!UNIX_SCRIPT.contains("runtime-*"));
        assert!(WINDOWS_SCRIPT.contains("WaitForExit"));
        assert!(WINDOWS_SCRIPT.contains("java.exe"));
        assert!(UNIX_SCRIPT.contains("kill -0"));
        assert!(UNIX_SCRIPT.contains("cloudstream.data.dir"));
    }
}
