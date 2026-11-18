//! Spawns the sidecar child and wires its stdin/stdout/stderr pipes.

use std::io::Write;
use std::sync::mpsc::{self, Receiver};

use extension_api::GatewayError;

use crate::sidecar_output::{forward_stdout, log_stderr};
use crate::SidecarLaunchConfig;

/// A running child process and the channel its reply lines arrive on.
pub(crate) struct RunningSidecar {
    pub(crate) pid: u32,
    pub(crate) stdin: Box<dyn Write + Send>,
    pub(crate) replies: Receiver<String>,
    #[cfg(windows)]
    _guards: WindowsGuards,
    #[cfg(not(windows))]
    child: std::process::Child,
}

/// Keeps the process and Job Object handles alive for the life of the sidecar.
#[cfg(windows)]
pub(crate) struct WindowsGuards {
    _process: WindowsProcess,
    _job: crate::sidecar_job_object::SidecarJobObject,
}

/// Either a std `Child` (tests / plain spawn) or an owned handle (AppContainer).
/// Held only so Drop keeps the process alive until the Job Object closes.
#[cfg(windows)]
enum WindowsProcess {
    Child(#[allow(dead_code)] std::process::Child),
    Handle(#[allow(dead_code)] std::os::windows::io::OwnedHandle),
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) fn start_sidecar(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    #[cfg(windows)]
    {
        start_windows(config)
    }
    #[cfg(not(windows))]
    {
        start_plain(config)
    }
}

#[cfg(windows)]
fn start_windows(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    if config_requests_app_container(config) {
        start_app_container(config)
    } else {
        start_windows_plain(config)
    }
}

/// Production JVM launches set `cloudstream.data.dir` via `with_data_dir`; tests use `fake_sidecar`.
#[cfg(windows)]
fn config_requests_app_container(config: &SidecarLaunchConfig) -> bool {
    config
        .environment
        .iter()
        .any(|(name, value)| name == "JAVA_OPTS" && value.contains("-Dcloudstream.data.dir="))
}

#[cfg(windows)]
fn start_app_container(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    use crate::sidecar_app_container_launch::start_in_app_container;

    let launched = start_in_app_container(config)?;
    wire_windows(WiredSidecar {
        pid: launched.pid,
        stdin: Box::new(launched.stdin),
        stdout: Box::new(launched.stdout),
        stderr: Box::new(launched.stderr),
        process: WindowsProcess::Handle(launched.process),
        job: launched.job,
    })
}

#[cfg(windows)]
fn start_windows_plain(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    use std::process::{Command, Stdio};

    use crate::sidecar_job_object::SidecarJobObject;

    let mut command = Command::new(&config.program);
    command
        .args(&config.args)
        .envs(config.environment.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|source| {
        GatewayError::Unavailable(format!(
            "cannot start {}: {source}",
            config.program.display()
        ))
    })?;
    let job = SidecarJobObject::constrain(&child)?;
    tracing::info!(program = %config.program.display(), "sidecar started");
    let missing_pipe = || GatewayError::Transport("sidecar pipe missing".to_owned());
    let stdin = child.stdin.take().ok_or_else(missing_pipe)?;
    let stdout = child.stdout.take().ok_or_else(missing_pipe)?;
    let stderr = child.stderr.take().ok_or_else(missing_pipe)?;
    wire_windows(WiredSidecar {
        pid: child.id(),
        stdin: Box::new(stdin),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        process: WindowsProcess::Child(child),
        job,
    })
}

#[cfg(windows)]
struct WiredSidecar {
    pid: u32,
    stdin: Box<dyn Write + Send>,
    stdout: Box<dyn std::io::Read + Send>,
    stderr: Box<dyn std::io::Read + Send>,
    process: WindowsProcess,
    job: crate::sidecar_job_object::SidecarJobObject,
}

#[cfg(windows)]
fn wire_windows(parts: WiredSidecar) -> Result<RunningSidecar, GatewayError> {
    let (sender, replies) = mpsc::channel();
    spawn_reader("sidecar-stdout", move || {
        forward_stdout(parts.stdout, &sender)
    })?;
    spawn_reader("sidecar-stderr", move || log_stderr(parts.stderr))?;
    Ok(RunningSidecar {
        pid: parts.pid,
        stdin: parts.stdin,
        replies,
        _guards: WindowsGuards {
            _process: parts.process,
            _job: parts.job,
        },
    })
}

#[cfg(not(windows))]
fn start_plain(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    use std::process::{Command, Stdio};

    let mut command = Command::new(&config.program);
    command
        .args(&config.args)
        .envs(config.environment.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    let pid = child.id();
    Ok(RunningSidecar {
        pid,
        stdin: Box::new(stdin),
        replies,
        child,
    })
}

fn spawn_reader(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), GatewayError> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map(drop)
        .map_err(|source| GatewayError::Transport(format!("cannot start {name} thread: {source}")))
}
