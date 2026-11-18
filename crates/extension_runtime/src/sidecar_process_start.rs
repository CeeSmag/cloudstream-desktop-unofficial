//! Spawns the sidecar child and wires its stdin/stdout/stderr pipes.

use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use extension_api::GatewayError;

use crate::sidecar_output::{forward_stdout, log_stderr};
use crate::SidecarLaunchConfig;

/// Windows flag that stops a console window from flashing up for the child.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A running child process and the channel its reply lines arrive on.
pub(crate) struct RunningSidecar {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) replies: Receiver<String>,
    /// Windows Job Object that caps memory and kills the tree when dropped.
    #[cfg(windows)]
    pub(crate) _job: crate::sidecar_job_object::SidecarJobObject,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) fn start_sidecar(config: &SidecarLaunchConfig) -> Result<RunningSidecar, GatewayError> {
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

    #[cfg(windows)]
    let job = match crate::sidecar_job_object::SidecarJobObject::constrain(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };

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
        #[cfg(windows)]
        _job: job,
    })
}

fn spawn_reader(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), GatewayError> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map(drop)
        .map_err(|source| GatewayError::Transport(format!("cannot start {name} thread: {source}")))
}
