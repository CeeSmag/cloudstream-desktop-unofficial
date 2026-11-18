//! Ends a sidecar process tree (Windows: `taskkill /T`; elsewhere: `kill -KILL`).

use std::process::{Child, Command, Stdio};

/// Windows flag that stops a console window from flashing up for the helper command.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Kills the process tree, then the child handle, then waits for reaping.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn discard_child(child: &mut Child) {
    kill_process_tree(child.id());
    if let Err(error) = child.kill() {
        tracing::debug!(%error, "sidecar already exited");
    }
    if let Err(error) = child.wait() {
        tracing::debug!(%error, "could not reap sidecar");
    }
}

/// On Windows the sidecar starts through `cs-sidecar.bat` (cmd.exe), which starts `java.exe`; ending
/// only the wrapper leaves the JVM running, holding the plugin files. `taskkill /T` ends the whole tree.
#[cfg(windows)]
pub(crate) fn kill_process_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("taskkill");
    command
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW);
    end_by_pid(command, pid);
}

/// Elsewhere the sidecar's script `exec`s the JVM, so the process is the JVM itself.
#[cfg(not(windows))]
pub(crate) fn kill_process_tree(pid: u32) {
    let mut command = Command::new("kill");
    command.args(["-KILL", &pid.to_string()]);
    end_by_pid(command, pid);
}

fn end_by_pid(mut command: Command, pid: u32) {
    let outcome = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
    match outcome {
        Ok(status) => tracing::info!(
            pid,
            success = status.success(),
            "ended the sidecar's process tree"
        ),
        Err(error) => {
            tracing::warn!(pid, %error, "cannot end the sidecar's process tree; it may keep running")
        }
    }
}
