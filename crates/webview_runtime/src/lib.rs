//! The Microsoft Edge WebView2 Runtime on Windows: whether it is there, and installing it with
//! Microsoft's own setup when it is not. The Cloudflare check opens its window in WebView2; without
//! the runtime it cannot open.
//!
//! The rules (what the registry values mean, which addresses may serve the setup, whose signature
//! counts) are pure functions. The PC is reached through traits ([`RuntimeProbe`], and for the
//! install `SetupFetcher`, `SignatureVerifier`, `SetupRunner`), implemented for Windows in
//! `windows/` and by fakes in the tests.
#![forbid(unsafe_code)]

mod http_fetcher;
mod install_gate;
mod install_phase;
mod runtime_installer;
mod runtime_probe;
mod runtime_status;
mod setup_error;
mod setup_folder;
mod setup_source;
mod signature_check;
mod status_board;
#[cfg(windows)]
mod windows;

use std::path::PathBuf;
use std::sync::Arc;

pub use http_fetcher::UreqSetupFetcher;
pub use install_gate::{CancelFlag, InstallGate, InstallRun};
pub use install_phase::{percent, InstallPhase, ProgressThrottle};
pub use runtime_installer::{
    outcome, FetchRequest, InstallerParts, RuntimeInstaller, SetupFetcher, SetupRunner,
    SignatureVerifier,
};
pub use runtime_probe::{platform_probe, pretended_facts, FixedProbe, RuntimeProbe};
pub use runtime_status::{judge, parse_version, RuntimeFacts, RuntimeStatus, MIN_WINDOWS_MAJOR};
pub use setup_error::SetupError;
pub use setup_folder::{AttemptFolder, SETUP_FILE, SETUP_FOLDER};
pub use setup_source::{
    host_of, is_allowed_setup_address, is_microsoft_host, next_hop, DOWNLOAD_PAGE, MAX_HOPS,
    MAX_SETUP_BYTES, SETUP_URL,
};
pub use signature_check::judge_signature;
pub use status_board::{ProbeTicket, StatusBoard};

/// The installer for this PC: Microsoft's setup from Microsoft's servers, its signature read by
/// Windows, the registry read again afterwards. `None` where there is nothing to install.
pub fn platform_installer(
    probe: Arc<dyn RuntimeProbe>,
    data_dir: PathBuf,
) -> Option<RuntimeInstaller> {
    #[cfg(windows)]
    {
        let parts = InstallerParts {
            fetcher: Arc::new(UreqSetupFetcher::microsoft()),
            verifier: Arc::new(windows::powershell_signature::PowerShellSignature),
            runner: Arc::new(windows::bootstrapper_runner::BootstrapperRunner),
            probe,
        };
        Some(RuntimeInstaller::new(parts, data_dir))
    }
    #[cfg(not(windows))]
    {
        let _ = (probe, data_dir);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_an_installer_on_windows_only() {
        let installer = platform_installer(platform_probe(), std::env::temp_dir());
        assert_eq!(installer.is_some(), cfg!(windows));
    }
}
