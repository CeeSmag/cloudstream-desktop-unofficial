//! The `cloudstream` executable.
#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use app_shell::{build_services, start_logging, AppPaths, ServiceOptions};

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::from_environment()?;
    let _debug_logs = start_logging(&paths.log_file());
    tracing::info!(data_dir = %paths.data_dir().display(), "cloudstream starting");
    let services = build_services(&paths, &ServiceOptions::default())?;
    ui_app::run_desktop_app(services).map_err(|error| anyhow::anyhow!("window failed: {error}"))
}
