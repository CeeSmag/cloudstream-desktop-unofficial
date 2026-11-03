use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use extension_api::GatewayError;
use serde_json::Value;

use crate::rpc_message::{encode_request, parse_reply};
use crate::{RpcTransport, SidecarLaunchConfig};

/// Windows flag that stops a console window from flashing up for the child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Longest sidecar output line kept in the log, in characters.
const MAX_LOGGED_LINE_CHARS: usize = 400;

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

fn discard(slot: &mut Option<RunningSidecar>) {
    if let Some(mut running) = slot.take() {
        if let Err(error) = running.child.kill() {
            tracing::debug!(%error, "sidecar already exited");
        }
        if let Err(error) = running.child.wait() {
            tracing::debug!(%error, "could not reap sidecar");
        }
    }
}

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

fn forward_stdout(stdout: impl std::io::Read, sender: &Sender<String>) {
    for line in lossy_lines(stdout) {
        if sender.send(line).is_err() {
            break;
        }
    }
}

/// Reads lines from the child's output and never stops at bytes that are not valid UTF-8:
/// extensions print text in the Windows code page, and a reader that quit there would leave the
/// pipe undrained, freezing the child as soon as the pipe filled up.
fn lossy_lines(source: impl std::io::Read) -> impl Iterator<Item = String> {
    let mut reader = BufReader::new(source);
    std::iter::from_fn(move || {
        let mut bytes = Vec::new();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) => None,
            Ok(_) => Some(
                String::from_utf8_lossy(&bytes)
                    .trim_end_matches(['\r', '\n'])
                    .to_owned(),
            ),
            Err(error) => {
                tracing::warn!(%error, "cannot read the sidecar output any more");
                None
            }
        }
    })
}

/// Everything the runtime and the extensions print ends up in the app log, so a failing
/// extension leaves a trace; lines that look like trouble are warnings.
fn log_stderr(stderr: impl std::io::Read) {
    for line in lossy_lines(stderr) {
        let line = shortened(&line);
        if looks_like_trouble(line) {
            tracing::warn!(target: "sidecar", "{line}");
        } else {
            tracing::info!(target: "sidecar", "{line}");
        }
    }
}

/// Extensions sometimes print whole web pages; the log keeps only the start of such a line.
fn shortened(line: &str) -> &str {
    match line.char_indices().nth(MAX_LOGGED_LINE_CHARS) {
        Some((end, _)) => &line[..end],
        None => line,
    }
}

fn looks_like_trouble(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "exception",
        "error",
        "failed",
        "cannot",
        "challenge",
        "timed out",
    ]
    .iter()
    .any(|word| lower.contains(word))
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

#[cfg(test)]
mod tests {
    use super::{looks_like_trouble, lossy_lines, shortened};

    #[test]
    fn lines_that_report_trouble_are_recognised() {
        assert!(looks_like_trouble(
            "java.lang.IllegalStateException: no provider"
        ));
        assert!(looks_like_trouble("[sidecar] request failed: boom"));
        assert!(looks_like_trouble(
            "[sidecar] Cloudflare challenge on site.test"
        ));
        assert!(!looks_like_trouble("[sidecar] ready"));
    }

    #[test]
    fn reading_goes_on_after_a_line_that_is_not_utf8() {
        let bytes: &[u8] = b"first\r\ncaf\xe9 au lait\nlast";
        let lines: Vec<String> = lossy_lines(bytes).collect();
        assert_eq!(lines.len(), 3, "the bad line must not end the reading");
        assert_eq!(lines[0], "first");
        assert!(lines[1].starts_with("caf") && lines[1].ends_with("au lait"));
        assert_eq!(lines[2], "last");
    }

    #[test]
    fn very_long_lines_are_cut_but_short_ones_are_kept() {
        let page = "<html>".repeat(500);
        assert_eq!(shortened(&page).chars().count(), 400);
        assert_eq!(shortened("[sidecar] ready"), "[sidecar] ready");
        assert_eq!(shortened("é".repeat(401).as_str()).chars().count(), 400);
    }
}
