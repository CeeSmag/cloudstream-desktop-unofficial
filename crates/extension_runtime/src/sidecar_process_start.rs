//! Spawns the sidecar child and wires its stdin/stdout/stderr pipes.

use std::io::Write;
use std::sync::mpsc::{self, Receiver};

use extension_api::GatewayError;

use crate::sidecar_output::{forward_stdout, log_stderr};
use crate::sidecar_spawn_mode::{uses_linux_process_limits, uses_os_fs_sandbox};
use crate::SidecarLaunchConfig;

/// A running child process and the channel its reply lines arrive on.
pub(crate) struct RunningSidecar {
    pub(crate) pid: u32,
    pub(crate) stdin: Box<dyn Write + Send>,
    pub(crate) replies: Receiver<String>,
    #[cfg(windows)]
    _guards: WindowsGuards,
    #[cfg(not(windows))]
    pub(crate) child: std::process::Child,
    /// Present for production Linux sandboxes; Drop kills the process group.
    #[cfg(target_os = "linux")]
    pub(crate) linux_guards: Option<crate::sidecar_linux_limits::LinuxGuards>,
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
    #[cfg(target_os = "linux")]
    {
        start_linux(config)
    }
    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        start_plain(config)
    }
}

#[cfg(windows)]
fn start_windows(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    if uses_os_fs_sandbox(config) {
        start_app_container(config)
    } else {
        // Trusted (or tests): Job Object only — no AppContainer.
        start_windows_plain(config)
    }
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
    tracing::info!(
        program = %config.program.display(),
        "sidecar started with Job Object (no AppContainer)"
    );
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

#[cfg(target_os = "linux")]
fn start_linux(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    if uses_os_fs_sandbox(config) {
        start_sandboxed(config)
    } else if uses_linux_process_limits(config) {
        start_limited(config)
    } else {
        start_plain(config)
    }
}

#[cfg(target_os = "linux")]
fn sandboxed_command(
    config: &SidecarLaunchConfig,
) -> Result<(std::process::Command, i32), GatewayError> {
    use std::process::{Command, Stdio};

    use crate::sidecar_linux_landlock::{attach_to_command, ensure_supported};
    use crate::sidecar_linux_paths::SandboxPaths;

    let abi = ensure_supported()?;
    let paths = SandboxPaths::from_config(config)?;
    let mut command = Command::new(&config.program);
    command
        .args(&config.args)
        .envs(config.environment.iter().map(|(k, v)| (k, v)))
        // Landlock has no X11 socket; clear DISPLAY so AWT never tries `:0`.
        .env_remove("DISPLAY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    attach_to_command(&mut command, paths);
    Ok((command, abi))
}

#[cfg(target_os = "linux")]
fn start_sandboxed(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    use crate::sidecar_linux_limits::{sandbox_unavailable, LinuxGuards, MEMORY_LIMIT_BYTES};

    let (mut command, abi) = sandboxed_command(config)?;
    let child = command.spawn().map_err(|source| {
        sandbox_unavailable(format!("spawn {}: {source}", config.program.display()))
    })?;
    let pid = child.id();
    let linux_guards = Some(LinuxGuards::for_leader(pid));
    tracing::info!(
        program = %config.program.display(),
        pid,
        landlock_abi = abi,
        memory_mb = MEMORY_LIMIT_BYTES / (1024 * 1024),
        "linux sandbox: landlock+rlimit+pgid"
    );
    finish_linux_child(child, linux_guards)
}

/// Trusted plugins: rlimit + process group, no Landlock FS jail.
#[cfg(target_os = "linux")]
fn start_limited(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
    use std::process::{Command, Stdio};

    use crate::sidecar_linux_limits::{
        attach_limits_to_command, sandbox_unavailable, LinuxGuards, MEMORY_LIMIT_BYTES,
    };

    let mut command = Command::new(&config.program);
    command
        .args(&config.args)
        .envs(config.environment.iter().map(|(k, v)| (k, v)))
        .env_remove("DISPLAY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    attach_limits_to_command(&mut command);
    let child = command.spawn().map_err(|source| {
        sandbox_unavailable(format!("spawn {}: {source}", config.program.display()))
    })?;
    let pid = child.id();
    let linux_guards = Some(LinuxGuards::for_leader(pid));
    tracing::info!(
        program = %config.program.display(),
        pid,
        memory_mb = MEMORY_LIMIT_BYTES / (1024 * 1024),
        "linux limits: rlimit+pgid (no Landlock)"
    );
    finish_linux_child(child, linux_guards)
}

#[cfg(target_os = "linux")]
fn finish_linux_child(
    mut child: std::process::Child,
    linux_guards: Option<crate::sidecar_linux_limits::LinuxGuards>,
) -> Result<RunningSidecar, GatewayError> {
    let pid = child.id();
    let missing_pipe = || GatewayError::Transport("sidecar pipe missing".to_owned());
    let stdin = child.stdin.take().ok_or_else(missing_pipe)?;
    let stdout = child.stdout.take().ok_or_else(missing_pipe)?;
    let stderr = child.stderr.take().ok_or_else(missing_pipe)?;
    let (sender, replies) = mpsc::channel();
    spawn_reader("sidecar-stdout", move || forward_stdout(stdout, &sender))?;
    spawn_reader("sidecar-stderr", move || log_stderr(stderr))?;
    Ok(RunningSidecar {
        pid,
        stdin: Box::new(stdin),
        replies,
        child,
        linux_guards,
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
        #[cfg(target_os = "linux")]
        linux_guards: None,
    })
}

fn spawn_reader(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), GatewayError> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map(drop)
        .map_err(|source| GatewayError::Transport(format!("cannot start {name} thread: {source}")))
}
