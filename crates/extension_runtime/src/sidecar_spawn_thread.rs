//! Starts sidecar processes from one long-lived thread.
//!
//! On Linux the sidecar asks for `PR_SET_PDEATHSIG(SIGKILL)` so that it dies with the app. The
//! kernel sends that signal when the *thread* that forked the child ends, not the process. The UI
//! calls plugins from short-lived worker threads, so a sidecar started on one of them was killed as
//! soon as that call's thread finished, and the next call paid a JVM start (seen in the log as
//! `exited on its own … unix_wait_status(9)` after nearly every call). Forking from this thread,
//! which lives as long as the pool that owns it, keeps the signal for what it is meant for.

use std::fmt;
use std::io;
use std::process::{Child, Command};
use std::sync::mpsc::{self, Sender};
use std::thread;

/// One process to start and where to hand it back.
struct SpawnRequest {
    command: Command,
    reply: Sender<io::Result<Child>>,
}

/// A thread that forks sidecar processes for whoever asks; it ends when the spawner is dropped.
pub struct SidecarSpawner {
    requests: Sender<SpawnRequest>,
}

impl fmt::Debug for SidecarSpawner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SidecarSpawner")
    }
}

impl SidecarSpawner {
    /// Starts the spawner thread.
    pub fn start() -> io::Result<Self> {
        let (requests, incoming) = mpsc::channel::<SpawnRequest>();
        thread::Builder::new()
            .name("sidecar-spawner".into())
            .spawn(move || {
                for mut request in incoming {
                    let started = request.command.spawn();
                    if let Err(unclaimed) = request.reply.send(started) {
                        // The caller is gone (it panicked while waiting): end the orphan.
                        if let Ok(mut child) = unclaimed.0 {
                            tracing::warn!(
                                pid = child.id(),
                                "nobody took the started sidecar; ending it"
                            );
                            if let Err(error) = child.kill() {
                                tracing::debug!(%error, "the orphan sidecar already ended");
                            }
                        }
                    }
                }
                tracing::debug!("sidecar spawner thread finished");
            })?;
        Ok(Self { requests })
    }

    /// Starts `command` on the spawner thread and returns the child.
    pub(crate) fn spawn(&self, command: Command) -> io::Result<Child> {
        let (reply, answer) = mpsc::channel();
        self.requests
            .send(SpawnRequest { command, reply })
            .map_err(|_| io::Error::other("the sidecar spawner thread is gone"))?;
        answer
            .recv()
            .map_err(|_| io::Error::other("the sidecar spawner thread stopped"))?
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;

    /// `sleep` that asks to be killed when its parent thread ends, as the real sidecar does.
    fn guarded_sleep() -> Command {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new("sleep");
        command.arg("30");
        // SAFETY: the closure runs in the forked child before exec and only calls prctl.
        #[allow(unsafe_code)]
        unsafe {
            command.pre_exec(|| {
                // SAFETY: PR_SET_PDEATHSIG with a valid signal number.
                let result = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong);
                if result == 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            });
        }
        command
    }

    /// Waits up to `limit` for the child to end; true when it ended.
    fn ends_within(child: &mut Child, limit: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < limit {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn spawn_from_short_lived_thread_without_spawner_is_killed() {
        // The old behaviour: the forking thread ends, the kernel kills the child.
        let mut child = thread::spawn(|| guarded_sleep().spawn().expect("spawn"))
            .join()
            .expect("join");
        assert!(
            ends_within(&mut child, Duration::from_secs(2)),
            "child survived its thread"
        );
    }

    #[test]
    fn spawn_via_spawner_from_short_lived_thread_keeps_child_alive() {
        let spawner = Arc::new(SidecarSpawner::start().expect("start"));
        let asking = Arc::clone(&spawner);
        let mut child = thread::spawn(move || asking.spawn(guarded_sleep()).expect("spawn"))
            .join()
            .expect("join");
        assert!(
            !ends_within(&mut child, Duration::from_millis(500)),
            "child was killed"
        );
        child.kill().expect("kill");
        let _ = child.wait();
    }

    #[test]
    fn spawn_concurrent_requests_each_caller_gets_its_own_child() {
        let spawner = Arc::new(SidecarSpawner::start().expect("start"));
        let callers: Vec<_> = (0..4)
            .map(|caller| {
                let spawner = Arc::clone(&spawner);
                thread::spawn(move || {
                    for round in 0..40 {
                        let code = (caller * 40 + round) % 250;
                        let mut command = Command::new("sh");
                        command.args(["-c", &format!("exit {code}")]);
                        let status = spawner.spawn(command).expect("spawn").wait().expect("wait");
                        assert_eq!(
                            status.code(),
                            Some(code),
                            "child handed to the wrong caller"
                        );
                    }
                })
            })
            .collect();
        let deadline = Instant::now() + Duration::from_secs(20);
        for caller in callers {
            assert!(Instant::now() < deadline, "callers did not finish in time");
            caller.join().expect("caller");
        }
    }

    #[test]
    fn spawn_after_spawner_thread_gone_reports_error() {
        let (requests, incoming) = mpsc::channel::<SpawnRequest>();
        drop(incoming);
        let spawner = SidecarSpawner { requests };
        assert!(spawner.spawn(Command::new("true")).is_err());
    }
}
