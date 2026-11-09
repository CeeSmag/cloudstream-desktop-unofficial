use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use extension_api::GatewayError;
use serde_json::Value;

use crate::rpc_message::{encode_request, parse_reply};
use crate::sidecar_output::{forward_stdout, log_stderr};
use crate::{RpcTransport, SidecarLaunchConfig};

/// Windows flag that stops a console window from flashing up for the child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A running child process and the channel its reply lines arrive on.
struct RunningSidecar {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<String>,
}

/// `RpcTransport` that talks JSON lines to a child process.
///
/// The child is started on first use, killed on timeout or transport failure,
/// and started again on the next call, so one bad call never wedges the app.
pub struct SidecarProcessTransport {
    config: SidecarLaunchConfig,
    running: Mutex<Option<RunningSidecar>>,
    next_request_id: AtomicU64,
    /// Calls repeated on every fresh start, for example loading the installed extensions.
    startup_calls: Mutex<Vec<StartupCall>>,
}

/// A call to repeat whenever a fresh sidecar is started.
struct StartupCall {
    method: String,
    params: Value,
    timeout: Duration,
}

impl SidecarProcessTransport {
    /// Creates a transport; nothing is started until the first call.
    pub fn new(config: SidecarLaunchConfig) -> Self {
        Self {
            config,
            running: Mutex::new(None),
            next_request_id: AtomicU64::new(1),
            startup_calls: Mutex::new(Vec::new()),
        }
    }

    /// Repeats the remembered startup calls on a sidecar that has just been started again.
    #[tracing::instrument(level = "debug", skip_all)]
    fn replay_startup_calls(&self, running: &mut RunningSidecar) {
        let calls = self
            .startup_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for call in calls.iter() {
            let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
            let pending = PendingCall {
                id,
                method: &call.method,
                params: &call.params,
                timeout: call.timeout,
            };
            match exchange(running, &pending) {
                Ok(_) => {
                    tracing::info!(method = %call.method, "repeated a startup call on the new sidecar")
                }
                Err(error) => {
                    tracing::warn!(method = %call.method, %error, "a startup call failed on the new sidecar")
                }
            }
        }
    }

    fn lock_running(&self) -> MutexGuard<'_, Option<RunningSidecar>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl RpcTransport for SidecarProcessTransport {
    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, GatewayError> {
        let mut slot = self.lock_running();
        if slot.is_none() {
            let mut fresh = start_sidecar(&self.config)?;
            self.replay_startup_calls(&mut fresh);
            *slot = Some(fresh);
        }
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let pending = PendingCall {
            id,
            method,
            params: &params,
            timeout,
        };
        let outcome = match slot.as_mut() {
            Some(running) => exchange(running, &pending),
            None => Err(GatewayError::Unavailable("sidecar vanished".to_owned())),
        };
        if matches!(
            outcome,
            Err(GatewayError::Timeout { .. } | GatewayError::Transport(_))
        ) {
            tracing::warn!(method, "discarding sidecar process after failed call");
            discard(&mut slot);
        }
        outcome
    }

    fn remember_setting_for_restart(&self, method: &str, params: Value, timeout: Duration) {
        let mut calls = self
            .startup_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        calls.retain(|call| call.method != method);
        calls.push(StartupCall {
            method: method.to_owned(),
            params,
            timeout,
        });
    }

    fn remember_for_restart(&self, method: &str, params: Value, timeout: Duration) {
        let mut calls = self
            .startup_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !calls
            .iter()
            .any(|call| call.method == method && call.params == params)
        {
            calls.push(StartupCall {
                method: method.to_owned(),
                params,
                timeout,
            });
        }
    }
}

impl Drop for SidecarProcessTransport {
    fn drop(&mut self) {
        discard(&mut self.lock_running());
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn discard(slot: &mut Option<RunningSidecar>) {
    if let Some(mut running) = slot.take() {
        kill_process_tree(running.child.id());
        if let Err(error) = running.child.kill() {
            tracing::debug!(%error, "sidecar already exited");
        }
        if let Err(error) = running.child.wait() {
            tracing::debug!(%error, "could not reap sidecar");
        }
    }
}

/// On Windows the sidecar starts through `cs-sidecar.bat` (cmd.exe), which starts `java.exe`; ending
/// only the wrapper leaves the JVM running, holding the plugin files. `taskkill /T` ends the whole tree.
#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let outcome = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    if let Err(error) = outcome {
        tracing::debug!(%error, "cannot end the sidecar's process tree");
    }
}

/// Elsewhere the sidecar's script `exec`s the JVM, so there is no wrapper to leave it behind.
#[cfg(not(windows))]
fn kill_process_tree(_pid: u32) {}

#[tracing::instrument(level = "debug", skip_all, err)]
fn start_sidecar(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    let mut command = Command::new(&config.program);
    command
        .args(&config.args)
        .envs(config.environment.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);

    let mut child = command.spawn().map_err(|source| {
        GatewayError::Unavailable(format!(
            "cannot start {}: {source}",
            config.program.display()
        ))
    })?;
    tracing::info!(program = %config.program.display(), "sidecar started");

    let missing_pipe = || GatewayError::Transport("sidecar pipe missing".to_owned());
    let stdin = child.stdin.take().ok_or_else(missing_pipe)?;
    let stdout = child.stdout.take().ok_or_else(missing_pipe)?;
    let stderr = child.stderr.take().ok_or_else(missing_pipe)?;
    let (sender, replies) = mpsc::channel();
    spawn_reader("sidecar-stdout", move || forward_stdout(stdout, &sender))?;
    spawn_reader("sidecar-stderr", move || log_stderr(stderr))?;
    Ok(RunningSidecar {
        child,
        stdin,
        replies,
    })
}

fn spawn_reader(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), GatewayError> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map(drop)
        .map_err(|source| GatewayError::Transport(format!("cannot start {name} thread: {source}")))
}

/// One request in flight.
struct PendingCall<'a> {
    id: u64,
    method: &'a str,
    params: &'a Value,
    timeout: Duration,
}

fn exchange(running: &mut RunningSidecar, call: &PendingCall<'_>) -> Result<Value, GatewayError> {
    let line = encode_request(call.id, call.method, call.params);
    writeln!(running.stdin, "{line}")
        .and_then(|()| running.stdin.flush())
        .map_err(|source| GatewayError::Transport(format!("cannot write request: {source}")))?;
    await_reply(&running.replies, call.id, call.method, call.timeout)
}

fn await_reply(
    replies: &Receiver<String>,
    id: u64,
    method: &str,
    timeout: Duration,
) -> Result<Value, GatewayError> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match replies.recv_timeout(remaining) {
            Ok(line) => match parse_reply(&line) {
                Ok(reply) if reply.id == Some(id) => return reply.into_value(),
                Ok(stale) => tracing::warn!(expected = id, got = ?stale.id, "ignoring stale reply"),
                Err(error) => tracing::warn!(%error, "ignoring unreadable sidecar line"),
            },
            Err(RecvTimeoutError::Timeout) => {
                return Err(GatewayError::Timeout {
                    method: method.to_owned(),
                    seconds: timeout.as_secs(),
                })
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(GatewayError::Transport(
                    "sidecar exited unexpectedly".to_owned(),
                ))
            }
        }
    }
}
