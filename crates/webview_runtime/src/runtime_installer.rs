//! Installing the runtime: download Microsoft's setup into a fresh folder, check that Microsoft
//! signed it, run it, then read the registry again. Success means "the runtime is there now",
//! not "the setup said 0": the setup also fails when the runtime was already installed, and can
//! succeed without installing anything.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::{
    judge, judge_signature, AttemptFolder, CancelFlag, InstallPhase, RuntimeProbe, RuntimeStatus,
    SetupError, SETUP_URL,
};

/// What a [`SetupFetcher`] is asked to download.
#[derive(Debug, Clone, Copy)]
pub struct FetchRequest<'a> {
    /// The address to start at (redirects are followed and each one is checked).
    pub url: &'a str,
    /// The file to write; it appears only once complete.
    pub dest: &'a Path,
    /// Raised when the viewer stops the install.
    pub cancel: &'a CancelFlag,
}

/// Downloads the setup.
pub trait SetupFetcher: Send + Sync {
    /// Writes `request.url` to `request.dest`, calling `progress(done, total)` as bytes arrive.
    fn fetch(
        &self,
        request: FetchRequest<'_>,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), SetupError>;
}

/// Reads a file's Authenticode signature as `Status|Subject` (see [`judge_signature`]).
pub trait SignatureVerifier: Send + Sync {
    /// The signature of `file`.
    fn describe(&self, file: &Path) -> Result<String, SetupError>;
}

/// Runs the setup and waits for it.
pub trait SetupRunner: Send + Sync {
    /// The setup's exit code.
    fn run(&self, file: &Path) -> Result<i32, SetupError>;
}

/// The PC-facing parts of an install.
#[derive(Clone)]
pub struct InstallerParts {
    /// Downloads the setup.
    pub fetcher: Arc<dyn SetupFetcher>,
    /// Reads its signature.
    pub verifier: Arc<dyn SignatureVerifier>,
    /// Runs it.
    pub runner: Arc<dyn SetupRunner>,
    /// Reads the registry afterwards.
    pub probe: Arc<dyn RuntimeProbe>,
}

/// Installs the WebView2 runtime with Microsoft's setup.
#[derive(Clone)]
pub struct RuntimeInstaller {
    parts: InstallerParts,
    data_dir: PathBuf,
    setup_url: String,
}

impl RuntimeInstaller {
    /// An installer that downloads into `<data_dir>/webview2-setup/`.
    pub fn new(parts: InstallerParts, data_dir: PathBuf) -> Self {
        Self {
            parts,
            data_dir,
            setup_url: SETUP_URL.to_owned(),
        }
    }

    /// The same installer starting from another address (tests).
    pub fn with_setup_url(mut self, url: impl Into<String>) -> Self {
        self.setup_url = url.into();
        self
    }

    /// Runs the whole install, telling `report` each phase (not the final `Done` / `Failed`: that
    /// is the result). The attempt's folder is gone when this returns.
    pub fn install(
        &self,
        cancel: &CancelFlag,
        report: &mut dyn FnMut(InstallPhase),
    ) -> Result<RuntimeStatus, SetupError> {
        let folder = AttemptFolder::create(&self.data_dir, SystemTime::now())?;
        let setup = folder.setup_file();
        self.download(&setup, cancel, report)?;
        stop_if_cancelled(cancel)?;
        report(InstallPhase::Verifying);
        judge_signature(&self.parts.verifier.describe(&setup)?)?;
        stop_if_cancelled(cancel)?;
        report(InstallPhase::Installing);
        tracing::info!("running Microsoft's WebView2 setup");
        let code = self.parts.runner.run(&setup)?;
        let status = judge(&self.parts.probe.facts());
        tracing::info!(code, ?status, "Microsoft's WebView2 setup finished");
        drop(folder);
        outcome(code, status)
    }

    fn download(
        &self,
        setup: &Path,
        cancel: &CancelFlag,
        report: &mut dyn FnMut(InstallPhase),
    ) -> Result<(), SetupError> {
        report(InstallPhase::Downloading { done: 0, total: 0 });
        let request = FetchRequest {
            url: &self.setup_url,
            dest: setup,
            cancel,
        };
        self.parts.fetcher.fetch(request, &mut |done, total| {
            report(InstallPhase::Downloading { done, total });
        })
    }
}

fn stop_if_cancelled(cancel: &CancelFlag) -> Result<(), SetupError> {
    if cancel.is_cancelled() {
        Err(SetupError::Cancelled)
    } else {
        Ok(())
    }
}

/// The install's result from the setup's exit code and what the registry says afterwards.
pub fn outcome(code: i32, status: RuntimeStatus) -> Result<RuntimeStatus, SetupError> {
    match status {
        RuntimeStatus::Installed { .. } | RuntimeStatus::NotNeeded => Ok(status),
        _ if code != 0 => Err(SetupError::InstallerFailed(code)),
        _ => Err(SetupError::StillMissing),
    }
}

#[cfg(test)]
#[path = "runtime_installer_tests.rs"]
mod tests;
