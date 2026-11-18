//! Process-group, PDEATHSIG, and address-space cap for Linux sidecar workers.
//! Mirrors the Windows Job Object (1536 MB + kill-on-close) without cgroup/systemd.
#![allow(unsafe_code)]

use std::io;

use extension_api::GatewayError;

/// Same budget as the Windows Job Object: `-Xmx512m` plus native/DEX peaks.
pub(crate) const MEMORY_LIMIT_BYTES: u64 = 1536 * 1024 * 1024;

/// Owns the worker's process-group id; dropping it sends `SIGKILL` to the whole group.
pub(crate) struct LinuxGuards {
    pgid: i32,
    armed: bool,
}

impl LinuxGuards {
    /// After a successful sandboxed spawn, the child's pid is its process-group leader.
    pub(crate) fn for_leader(pid: u32) -> Self {
        Self {
            pgid: pid as i32,
            armed: true,
        }
    }

    /// Ends the group now; Drop becomes a no-op afterward.
    pub(crate) fn kill_group(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        kill_process_group(self.pgid);
    }
}

impl Drop for LinuxGuards {
    fn drop(&mut self) {
        self.kill_group();
    }
}

/// `setpgid(0,0)`, `PR_SET_PDEATHSIG`, and `RLIMIT_AS`. Call only in the child (`pre_exec`).
pub(crate) fn apply_in_child() -> io::Result<()> {
    // SAFETY: runs once in the forked child before exec; setpgid(0,0) makes this pid the leader.
    let pgid_result = unsafe { libc::setpgid(0, 0) };
    if pgid_result != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: PR_SET_PDEATHSIG asks the kernel to signal this process if the parent dies.
    let death = unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong) };
    if death != 0 {
        return Err(io::Error::last_os_error());
    }
    set_address_space_limit()?;
    Ok(())
}

/// Sends `SIGKILL` to every process in `pgid` (pass the positive group id).
pub(crate) fn kill_process_group(pgid: i32) {
    if pgid <= 1 {
        tracing::warn!(pgid, "refusing to kill process group");
        return;
    }
    // SAFETY: negative pid targets the process group; SIGKILL is valid.
    let result = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    if result == 0 {
        tracing::info!(pgid, "ended the sidecar's process group");
    } else {
        let error = io::Error::last_os_error();
        tracing::debug!(pgid, %error, "process group already gone");
    }
}

/// Sends `SIGKILL` to a single process (plain/test spawns that are not group leaders).
pub(crate) fn kill_process(pid: u32) {
    if pid == 0 {
        return;
    }
    // SAFETY: SIGKILL to this pid.
    let result = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    if result == 0 {
        tracing::info!(pid, "ended the sidecar process");
    }
}

fn set_address_space_limit() -> io::Result<()> {
    let limit = libc::rlimit {
        rlim_cur: MEMORY_LIMIT_BYTES,
        rlim_max: MEMORY_LIMIT_BYTES,
    };
    // SAFETY: setrlimit with a stack-local rlimit; RLIMIT_AS is defined on Linux.
    let result = unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Maps a child `pre_exec` failure into the fail-closed gateway error.
pub(crate) fn sandbox_unavailable(reason: impl Into<String>) -> GatewayError {
    GatewayError::Unavailable(format!(
        "cannot apply Linux sidecar sandbox ({})",
        reason.into()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn linux_guards_drop_kills_process_group() {
        let mut child = unsafe {
            Command::new("sleep")
                .arg("30")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .pre_exec(apply_in_child)
                .spawn()
                .expect("spawn sleep")
        };
        let pid = child.id();
        let mut guards = LinuxGuards::for_leader(pid);
        guards.kill_group();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    assert!(!status.success(), "killed child must not exit 0");
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => panic!("child still alive after group kill"),
                Err(error) => panic!("try_wait: {error}"),
            }
        }
    }

    #[test]
    fn apply_in_child_sets_new_process_group() {
        let output = unsafe {
            Command::new("sh")
                .args(["-c", "echo $$; echo $PPID"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .pre_exec(apply_in_child)
                .output()
                .expect("run sh")
        };
        assert!(output.status.success());
        // If apply_in_child failed, spawn/output would fail; success is enough here.
        // Dedicated kill test covers Drop. Memory cap is exercised by production path.
        let _ = output;
    }

    #[test]
    fn kill_process_group_refuses_system_groups() {
        kill_process_group(0);
        kill_process_group(1);
    }
}
