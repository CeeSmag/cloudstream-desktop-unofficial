//! Running Microsoft's setup: `/silent /install`, without elevation (it then installs for this
//! user, which is why the per-user registry key is read too), and waiting for it with a cap.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::{SetupError, SetupRunner};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The setup downloads the runtime itself (~150 MB); on a slow line that takes a while.
const SETUP_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const POLL: Duration = Duration::from_millis(250);

/// Runs the setup as a child process.
pub(crate) struct BootstrapperRunner;

impl SetupRunner for BootstrapperRunner {
    fn run(&self, file: &Path) -> Result<i32, SetupError> {
        let mut child = Command::new(file)
            .args(["/silent", "/install"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| SetupError::disk("start", file, &error))?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status.code().unwrap_or(-1)),
                Ok(None) if started.elapsed() < SETUP_TIMEOUT => std::thread::sleep(POLL),
                Ok(None) => {
                    // Left running: stopping Microsoft's setup half-way could break the runtime.
                    tracing::warn!("Microsoft's WebView2 setup is still running after 15 minutes");
                    return Err(SetupError::TimedOut);
                }
                Err(error) => return Err(SetupError::disk("wait for", file, &error)),
            }
        }
    }
}
