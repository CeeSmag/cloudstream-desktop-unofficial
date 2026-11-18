//! Ends a sidecar process tree (Windows: `taskkill /T`; Linux: process group; else `kill -KILL`).

#[cfg(any(windows, all(not(windows), not(target_os = "linux"))))]
use std::process::{Command, Stdio};

use crate::sidecar_process_start::RunningSidecar;

/// Windows flag that stops a console window from flashing up for the helper command.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// If the child has already exited, reaps it and clears `slot`. Returns whether the slot was cleared.
#[cfg(not(windows))]
pub(crate) fn take_if_exited(slot: &mut Option<RunningSidecar>) -> bool {
    let Some(running) = slot.as_mut() else {
        return false;
    };
    let exited = match running.child.try_wait() {
        Ok(Some(status)) => {
            tracing::warn!(
                pid = running.pid,
                ?status,
                "sidecar process exited on its own; reaping"
            );
            true
        }
        Ok(None) => false,
        Err(error) => {
            tracing::warn!(pid = running.pid, %error, "could not poll sidecar process; discarding");
            true
        }
    };
    if exited {
        discard_running_slot(slot);
    }
    exited
}

#[cfg(windows)]
pub(crate) fn take_if_exited(slot: &mut Option<RunningSidecar>) -> bool {
    let _ = slot;
    false
}

fn discard_running_slot(slot: &mut Option<RunningSidecar>) {
    if let Some(mut running) = slot.take() {
        discard_running(&mut running);
    }
}

/// Kills the process tree for a running sidecar and drops its handles.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn discard_running(running: &mut RunningSidecar) {
    #[cfg(target_os = "linux")]
    {
        if let Some(guards) = running.linux_guards.as_mut() {
            guards.kill_group();
        } else {
            kill_process_tree(running.pid);
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        kill_process_tree(running.pid);
    }
    #[cfg(not(windows))]
    {
        if let Err(error) = running.child.kill() {
            tracing::debug!(%error, "sidecar already exited");
        }
        if let Err(error) = running.child.wait() {
            tracing::debug!(%error, "could not reap sidecar");
        }
    }
}

/// On Windows the sidecar may still leave helper processes; `taskkill /T` ends the whole tree.
#[cfg(windows)]
pub(crate) fn kill_process_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("taskkill");
    command
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW);
    end_by_pid(command, pid);
}

/// On Linux, sandboxed workers are process-group leaders (pgid == pid); plain/test
/// spawns are not, so always SIGKILL the pid itself after trying the group.
#[cfg(target_os = "linux")]
pub(crate) fn kill_process_tree(pid: u32) {
    crate::sidecar_linux_limits::kill_process_group(pid as i32);
    crate::sidecar_linux_limits::kill_process(pid);
}

/// Elsewhere the sidecar's script `exec`s the JVM, so the process is the JVM itself.
#[cfg(all(not(windows), not(target_os = "linux")))]
pub(crate) fn kill_process_tree(pid: u32) {
    let mut command = Command::new("kill");
    command.args(["-KILL", &pid.to_string()]);
    end_by_pid(command, pid);
}

#[cfg(any(windows, all(not(windows), not(target_os = "linux"))))]
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
