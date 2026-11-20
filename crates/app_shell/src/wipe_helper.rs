//! Detached post-exit helper for "Delete all saved data".
//!
//! On Windows a running executable (such as the portable launcher's unpacked
//! `runtime-<ver>-<id>\cloudstream.exe`) and its open DLLs, logs, and SQLite files cannot be
//! deleted in-process. Writing a self-deleting helper script into the OS temp directory (outside
//! `data_dir`) lets CloudStream shut down cleanly and then removes the entire `data_dir` tree so
//! only the standalone portable executable remains.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::data_wipe::running_folder_name;

/// Prefix of the portable launcher's unpacked program folder inside `data_dir`.
const RUNTIME_FOLDER_PREFIX: &str = "runtime-";

/// Maximum seconds the helper waits for CloudStream to exit before attempting cleanup anyway.
const MAX_WAIT_SECONDS: u32 = 30;

/// Maximum deletion passes when an antivirus scanner or OS handle briefly delays removal.
const MAX_DELETE_ATTEMPTS: u32 = 20;

/// Verifies that `data_dir` is safe to remove completely without touching a drive root or any
/// program binary living outside a portable `runtime-*` folder.
pub fn validate_wipe_target(
    data_dir: &Path,
    portable_exe: Option<&Path>,
    current_exe: Option<&Path>,
) -> io::Result<()> {
    if data_dir.parent().is_none() || data_dir.file_name().is_none() {
        return Err(invalid_target(data_dir, "refusing to delete a root folder"));
    }
    ensure_safe_path_text(data_dir)?;
    if let Some(portable) = portable_exe {
        if contains_path(data_dir, portable) {
            return Err(invalid_target(
                data_dir,
                "data folder contains the portable executable",
            ));
        }
    }
    if let Some(exe) = current_exe {
        let is_unpacked_runtime = running_folder_name(data_dir, Some(exe))
            .is_some_and(|name| name.starts_with(RUNTIME_FOLDER_PREFIX));
        if contains_path(data_dir, exe) && !is_unpacked_runtime {
            return Err(invalid_target(
                data_dir,
                "data folder contains the installed program",
            ));
        }
    }
    Ok(())
}

/// Validates `data_dir` and spawns the detached OS helper in the system temp folder to remove
/// `data_dir` once the current process exits.
pub fn spawn_wipe_helper(data_dir: &Path) -> io::Result<()> {
    let portable_exe = std::env::var_os("CS_PORTABLE_EXE").map(PathBuf::from);
    let current_exe = std::env::current_exe().ok();
    validate_wipe_target(data_dir, portable_exe.as_deref(), current_exe.as_deref())?;
    let script_path =
        spawn_wipe_helper_for_pid(std::process::id(), data_dir, &std::env::temp_dir())?;
    tracing::info!(
        data_dir = %data_dir.display(),
        helper = %script_path.display(),
        "started post-exit data wipe helper"
    );
    Ok(())
}

/// Writes the self-deleting wipe helper into `script_dir` and starts it detached for `pid`.
pub(crate) fn spawn_wipe_helper_for_pid(
    pid: u32,
    data_dir: &Path,
    script_dir: &Path,
) -> io::Result<PathBuf> {
    std::fs::create_dir_all(script_dir)?;
    #[cfg(windows)]
    {
        spawn_windows_helper(pid, data_dir, script_dir)
    }
    #[cfg(not(windows))]
    {
        spawn_unix_helper(pid, data_dir, script_dir)
    }
}

#[cfg(windows)]
fn spawn_windows_helper(pid: u32, data_dir: &Path, script_dir: &Path) -> io::Result<PathBuf> {
    use std::os::windows::process::CommandExt;

    /// Windows process flag: run cmd.exe without flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let name = format!("cloudstream-wipe-{pid}.bat");
    let script_path = script_dir.join(&name);
    let body = windows_wipe_script(pid, data_dir)?;
    std::fs::write(&script_path, body)?;
    // Run by plain file name inside `script_dir` so cmd never re-parses the user's temp path.
    Command::new("cmd.exe")
        .args(["/d", "/c", &format!(r".\{name}")])
        .current_dir(script_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(script_path)
}

#[cfg(not(windows))]
fn spawn_unix_helper(pid: u32, data_dir: &Path, script_dir: &Path) -> io::Result<PathBuf> {
    let name = format!("cloudstream-wipe-{pid}.sh");
    let script_path = script_dir.join(&name);
    let body = unix_wipe_script(pid, data_dir)?;
    std::fs::write(&script_path, body)?;
    Command::new("sh")
        .arg(&name)
        .current_dir(script_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(script_path)
}

/// Windows `.bat` helper: waits for `pid` to exit, deletes `data_dir` with retries, then removes
/// itself. Uses full System32 tool paths so Git/Unix `find.exe` on PATH cannot break the wait.
#[cfg(any(windows, test))]
pub(crate) fn windows_wipe_script(pid: u32, data_dir: &Path) -> io::Result<String> {
    let target = bat_quote(data_dir)?;
    let tools = r"%SystemRoot%\System32";
    Ok(format!(
        "@echo off\r\n\
         set /a waited=0\r\n\
         :wait\r\n\
         \"{tools}\\PING.EXE\" -n 2 127.0.0.1 >nul\r\n\
         \"{tools}\\tasklist.exe\" /fi \"PID eq {pid}\" | \"{tools}\\find.exe\" \"{pid}\" >nul\r\n\
         if not errorlevel 1 (\r\n\
           set /a waited+=1\r\n\
           if %waited% lss {MAX_WAIT_SECONDS} goto wait\r\n\
         )\r\n\
         set /a tries=0\r\n\
         :wipe\r\n\
         if not exist {target} goto done\r\n\
         rd /s /q {target} >nul 2>&1\r\n\
         if not exist {target} goto done\r\n\
         set /a tries+=1\r\n\
         if %tries% geq {MAX_DELETE_ATTEMPTS} goto done\r\n\
         \"{tools}\\PING.EXE\" -n 2 127.0.0.1 >nul\r\n\
         goto wipe\r\n\
         :done\r\n\
         (goto) 2>nul & del \"%~f0\"\r\n"
    ))
}

/// POSIX `sh` helper: waits for `pid` to exit, deletes `data_dir` with retries, then removes itself.
#[cfg(any(not(windows), test))]
pub(crate) fn unix_wipe_script(pid: u32, data_dir: &Path) -> io::Result<String> {
    let target = sh_quote(data_dir)?;
    let wait_steps = MAX_WAIT_SECONDS * 2;
    Ok(format!(
        "#!/bin/sh\n\
         waited=0\n\
         while kill -0 {pid} 2>/dev/null && [ \"$waited\" -lt {wait_steps} ]; do\n\
           sleep 0.5\n\
           waited=$((waited + 1))\n\
         done\n\
         tries=0\n\
         while [ -e {target} ] && [ \"$tries\" -lt {MAX_DELETE_ATTEMPTS} ]; do\n\
           rm -rf {target} 2>/dev/null || true\n\
           [ ! -e {target} ] && break\n\
           tries=$((tries + 1))\n\
           sleep 0.5\n\
         done\n\
         rm -f \"$0\"\n"
    ))
}

fn ensure_safe_path_text(path: &Path) -> io::Result<&str> {
    path.to_str()
        .filter(|text| !text.contains('"') && !text.chars().any(char::is_control))
        .ok_or_else(|| invalid_target(path, "path contains unsupported characters"))
}

#[cfg(any(windows, test))]
fn bat_quote(path: &Path) -> io::Result<String> {
    let text = ensure_safe_path_text(path)?;
    Ok(format!("\"{}\"", text.replace('%', "%%")))
}

#[cfg(any(not(windows), test))]
fn sh_quote(path: &Path) -> io::Result<String> {
    let text = ensure_safe_path_text(path)?;
    Ok(format!("'{}'", text.replace('\'', r"'\''")))
}

fn contains_path(container: &Path, candidate: &Path) -> bool {
    if candidate.starts_with(container) {
        return true;
    }
    match (
        std::fs::canonicalize(container),
        std::fs::canonicalize(candidate),
    ) {
        (Ok(canon_container), Ok(canon_candidate)) => canon_candidate.starts_with(canon_container),
        _ => false,
    }
}

fn invalid_target(path: &Path, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{reason}: {}", path.display()),
    )
}

#[cfg(test)]
#[path = "wipe_helper_tests.rs"]
mod tests;
