//! Starts the verification helper (`cloudstream-verify`) on demand and talks to it.
//!
//! The helper is a separate process because its browser windows (WebView2 / WebKitGTK) run their
//! own event loop, which must not run inside the app's Slint loop. It reads [`HelperCommand`]s on
//! stdin, prints [`HelperStatus`] lines on stdout, answers the sidecar's files itself, and exits
//! when its stdin closes (so it never outlives the app).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::helper_protocol::{format_command, parse_status, HelperCommand, HelperStatus};

/// How to start the helper.
#[derive(Debug, Clone)]
pub struct HelperLaunch {
    /// The helper program.
    pub program: PathBuf,
    /// Its arguments (the exchange folder and the browser profile folder).
    pub args: Vec<String>,
}

struct RunningHelper {
    child: Child,
    stdin: ChildStdin,
}

/// The helper process, started when first needed.
pub struct VerifyHelper {
    launch: HelperLaunch,
    running: Option<RunningHelper>,
    statuses: Sender<HelperStatus>,
    debug_logging: Arc<AtomicBool>,
}

fn write_command(child: &mut RunningHelper, command: &HelperCommand) -> std::io::Result<()> {
    let line = format_command(command);
    #[cfg(windows)]
    let line = {
        let mut line = line;
        let target = match command {
            HelperCommand::Present(request) => Some((&request.host, &request.id)),
            HelperCommand::Show { host, id } => Some((host, id)),
            _ => None,
        };
        if let Some((host, id)) = target {
            let pid = child.child.id();
            let granted = winsafe::AllowSetForegroundWindow(Some(pid)).is_ok();
            tracing::debug!(pid, %id, granted, "verification foreground grant");
            if granted {
                line.push_str(&format_command(&HelperCommand::Activate {
                    host: host.clone(),
                    id: id.clone(),
                }));
            }
        }
        line
    };
    child.stdin.write_all(line.as_bytes())?;
    child.stdin.flush()
}

impl VerifyHelper {
    /// Nothing starts until the first command.
    pub fn new(
        launch: HelperLaunch,
        statuses: Sender<HelperStatus>,
        debug_logging: Arc<AtomicBool>,
    ) -> Self {
        Self {
            launch,
            running: None,
            statuses,
            debug_logging,
        }
    }

    /// True while the helper process runs.
    pub fn is_running(&mut self) -> bool {
        let exited = match self
            .running
            .as_mut()
            .map(|running| running.child.try_wait())
        {
            Some(Ok(Some(status))) => {
                tracing::warn!(%status, "the verification helper exited");
                true
            }
            Some(Err(error)) => {
                tracing::warn!(%error, "cannot tell whether the verification helper runs");
                true
            }
            Some(Ok(None)) | None => false,
        };
        if exited {
            self.running = None;
        }
        self.running.is_some()
    }

    /// Sends `command`, starting the helper if needed and once more if the pipe broke.
    pub fn send(&mut self, command: &HelperCommand) -> std::io::Result<()> {
        for attempt in 0..2 {
            if !self.is_running() {
                self.start()?;
            }
            let written = match self.running.as_mut() {
                Some(running) => write_command(running, command),
                None => Err(std::io::Error::other(
                    "the verification helper did not start",
                )),
            };
            match written {
                Ok(()) => return Ok(()),
                Err(error) if attempt == 0 => {
                    tracing::warn!(%error, "the verification helper stopped listening; restarting it");
                    self.stop();
                }
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::other(
            "the verification helper did not take the command",
        ))
    }

    pub(crate) fn show_if_running(&mut self, host: &str, id: &str) -> std::io::Result<()> {
        if let Some(child) = self.running.as_mut() {
            write_command(
                child,
                &HelperCommand::Show {
                    host: host.into(),
                    id: id.into(),
                },
            )?;
        }
        Ok(())
    }

    fn start(&mut self) -> std::io::Result<()> {
        let mut command = Command::new(&self.launch.program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command.args(&self.launch.args);
        if self.debug_logging.load(Ordering::SeqCst) {
            command.arg("--debug");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let missing = || std::io::Error::other("the verification helper has no pipes");
        let stdin = child.stdin.take().ok_or_else(missing)?;
        let stdout = child.stdout.take().ok_or_else(missing)?;
        let statuses = self.statuses.clone();
        std::thread::Builder::new()
            .name("cloudflare-helper-out".into())
            .spawn(move || forward_statuses(stdout, &statuses))?;
        tracing::info!(pid = child.id(), "verification helper started");
        self.running = Some(RunningHelper { child, stdin });
        Ok(())
    }

    /// Asks the helper to quit and ends it.
    pub fn stop(&mut self) {
        if let Some(mut running) = self.running.take() {
            let quit = format_command(&HelperCommand::Quit);
            if running.stdin.write_all(quit.as_bytes()).is_err() {
                tracing::debug!("the verification helper was already gone");
            }
            drop(running.stdin);
            if let Err(error) = running.child.kill() {
                tracing::debug!(%error, "the verification helper had already exited");
            }
            let _reaped = running.child.wait();
        }
    }
}

impl Drop for VerifyHelper {
    fn drop(&mut self) {
        self.stop();
    }
}

fn forward_statuses(stdout: std::process::ChildStdout, statuses: &Sender<HelperStatus>) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else {
            break;
        };
        match parse_status(&line) {
            Some(status) => {
                if statuses.send(status).is_err() {
                    break;
                }
            }
            None => log_helper_line(&line),
        }
    }
}

fn log_helper_line(line: &str) {
    if line.contains(" ERROR ") {
        tracing::error!(%line, "verification helper");
    } else if line.contains(" WARN ") {
        tracing::warn!(%line, "verification helper");
    } else if line.contains(" DEBUG ") {
        tracing::debug!(%line, "verification helper");
    } else {
        tracing::info!(%line, "verification helper");
    }
}
