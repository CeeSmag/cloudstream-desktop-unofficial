//! The text of the helper scripts that install an update after the app has exited. Paths are
//! quoted so that any folder name the user may have (spaces, `%`, `&`, `'`, `$`) reaches the command
//! as it is: in a `.bat`, `%` is doubled (cmd expands it even inside quotes) and a path with `"` is
//! refused (Windows forbids it in names anyway); in `sh`, paths are single-quoted.

use std::path::Path;
use std::process::Command;

use crate::update_error::UpdateError;

/// File name of the Windows helper inside the update attempt's folder.
#[cfg(any(windows, test))]
pub(crate) const BAT_SCRIPT_NAME: &str = "install.bat";

/// File name of the Linux/macOS helper inside the update attempt's folder.
#[cfg(any(not(windows), test))]
pub(crate) const SH_SCRIPT_NAME: &str = "install.sh";

/// The command that runs the `.bat` helper in `folder`. cmd parses its own command line again (`&`
/// ends a command, `%NAME%` expands), and the folder holds the user's name, so the script is started
/// by its plain name from inside the folder and its full path never reaches that line.
#[cfg(any(windows, test))]
pub(crate) fn bat_command(folder: &Path) -> Command {
    let mut command = Command::new("cmd.exe");
    // `.\` because cmd does not look in the current folder when NoDefaultCurrentDirectoryInExePath
    // is set (some PCs and tools set it).
    command
        .args(["/d", "/c", &format!(r".\{BAT_SCRIPT_NAME}")])
        .current_dir(folder);
    command
}

/// The command that runs the `sh` helper in `folder`, started the same way as the `.bat` one.
#[cfg(any(not(windows), test))]
pub(crate) fn sh_command(folder: &Path) -> Command {
    let mut command = Command::new("sh");
    command.arg(SH_SCRIPT_NAME).current_dir(folder);
    command
}

/// A path for a `.bat` line, in double quotes.
#[cfg(any(windows, test))]
fn bat_quote(path: &Path) -> Result<String, UpdateError> {
    let text = path
        .to_str()
        .filter(|text| !text.contains('"') && !text.chars().any(char::is_control))
        .ok_or_else(|| UpdateError::UnsafePath(path.to_path_buf()))?;
    Ok(format!("\"{}\"", text.replace('%', "%%")))
}

/// A path for an `sh` line, in single quotes (a `'` inside becomes `'\''`).
#[cfg(any(not(windows), test))]
fn sh_quote(path: &Path) -> Result<String, UpdateError> {
    let text = path
        .to_str()
        .ok_or_else(|| UpdateError::UnsafePath(path.to_path_buf()))?;
    Ok(format!("'{}'", text.replace('\'', r"'\''")))
}

/// Lines that wait until process `pid` has ended. `ping` waits one second where `timeout` would not:
/// the helper runs without a console, and `timeout` exits at once without one. The tools are named
/// by their full path: with Git's tools early on PATH, `find` would be Git's, fail, and end the wait
/// while the app still runs.
#[cfg(any(windows, test))]
fn bat_wait_for(pid: u32) -> String {
    let tools = r"%SystemRoot%\System32";
    format!(
        ":wait\r\n\
         \"{tools}\\PING.EXE\" -n 2 127.0.0.1 >nul\r\n\
         \"{tools}\\tasklist.exe\" /fi \"PID eq {pid}\" | \"{tools}\\find.exe\" \"{pid}\" >nul\r\n\
         if not errorlevel 1 goto wait\r\n"
    )
}

/// The last line of a `.bat` helper: deletes the script itself. `(goto)` first leaves the script, so
/// cmd does not end with "The batch file cannot be found" (an error status) once the file is gone.
#[cfg(any(windows, test))]
const BAT_DELETE_ITSELF: &str = "(goto) 2>nul & del \"%~f0\"\r\n";

/// Windows, portable exe: waits for `pid` to end, moves `downloaded` over `target`, starts it.
#[cfg(any(windows, test))]
pub(crate) fn windows_replace_script(
    pid: u32,
    downloaded: &Path,
    target: &Path,
) -> Result<String, UpdateError> {
    let (downloaded, target) = (bat_quote(downloaded)?, bat_quote(target)?);
    Ok(format!(
        "@echo off\r\n{wait}\
         move /y {downloaded} {target}\r\n\
         start \"\" {target}\r\n{delete}",
        wait = bat_wait_for(pid),
        delete = BAT_DELETE_ITSELF
    ))
}

/// Windows, installed app: waits for `pid` to end, then runs the downloaded installer passively
/// (`/P`: progress only), as an update (`/UPDATE`), starting the app when done (`/R`), and removes it.
#[cfg(any(windows, test))]
pub(crate) fn windows_installer_script(pid: u32, installer: &Path) -> Result<String, UpdateError> {
    let installer = bat_quote(installer)?;
    Ok(format!(
        "@echo off\r\n{wait}\
         start \"\" /wait {installer} /P /R /UPDATE\r\n\
         del {installer}\r\n{delete}",
        wait = bat_wait_for(pid),
        delete = BAT_DELETE_ITSELF
    ))
}

/// Linux/macOS: waits for `pid` to end, moves `downloaded` over `target`, starts it.
#[cfg(any(not(windows), test))]
pub(crate) fn unix_replace_script(
    pid: u32,
    downloaded: &Path,
    target: &Path,
) -> Result<String, UpdateError> {
    let (downloaded, target) = (sh_quote(downloaded)?, sh_quote(target)?);
    Ok(format!(
        "#!/bin/sh\n\
         while kill -0 {pid} 2>/dev/null; do sleep 0.5; done\n\
         sleep 0.5\n\
         chmod +x {downloaded} 2>/dev/null || true\n\
         mv -f {downloaded} {target} || cp -f {downloaded} {target}\n\
         chmod +x {target} 2>/dev/null || true\n\
         unset APPDIR\n\
         nohup {target} >/dev/null 2>&1 &\n\
         rm -f \"$0\"\n"
    ))
}

#[cfg(test)]
#[path = "update_helper_script_tests.rs"]
mod tests;
