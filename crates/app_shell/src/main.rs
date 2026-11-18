//! The `cloudstream` executable.
#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use app_shell::{
    build_services, start_logging, wipe_if_requested, AppPaths, RetryPlan, ServiceOptions,
};
use ui_slint::{run_desktop_app_with, AppExtras};

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::from_environment()?;
    // First of all: a requested "delete all saved data" runs before anything is opened.
    let wipe = wipe_if_requested(paths.data_dir(), RetryPlan::default());
    let _debug_logs = start_logging(&paths.log_file());
    wipe.log();
    tracing::info!(data_dir = %paths.data_dir().display(), "cloudstream starting");
    let services = build_services(&paths, &ServiceOptions::default())?;
    let cloudflare_dir = paths.data_dir().join("sidecar").join("cloudflare");
    // Opens challenges only after the viewer allows the host; cookie return via UI confirm.
    let cloudflare = Some(cloudflare_bridge::spawn_watcher(cloudflare_dir, false));
    let extras = AppExtras {
        cloudflare,
        data_dir: Some(paths.data_dir().to_path_buf()),
    };
    run_desktop_app_with(services, extras)
        .map_err(|error| anyhow::anyhow!("window failed: {error}"))
}
