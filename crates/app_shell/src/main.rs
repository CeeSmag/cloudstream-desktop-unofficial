//! The `cloudstream` executable.
#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use app_shell::{
    build_services, request_wipe, start_cloudflare_watch, start_logging, wipe_if_requested,
    AppPaths, RetryPlan, ServiceOptions,
};
use ui_slint::{run_desktop_app_with, AppExtras, WipeRequest};

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::from_environment()?;
    // First of all: a requested "delete all saved data" runs before anything is opened.
    let wipe = wipe_if_requested(paths.data_dir(), RetryPlan::default());
    let debug_logs = start_logging(&paths.log_file());
    wipe.log();
    tracing::info!(data_dir = %paths.data_dir().display(), "cloudstream starting");
    let services = build_services(&paths, &ServiceOptions::default())?;
    let updates_dir = paths.data_dir().to_path_buf();
    if let Err(error) = std::thread::Builder::new()
        .name("update-cleanup".into())
        .spawn(move || app_updater::remove_stale_downloads(&updates_dir))
    {
        tracing::warn!(%error, "old update downloads are kept this run");
    }
    // Verification windows open only for sites the viewer allowed (Verify in the app).
    let cloudflare = Some(start_cloudflare_watch(paths.data_dir(), &services.settings));
    let extras = AppExtras {
        cloudflare,
        data_dir: Some(paths.data_dir().to_path_buf()),
        request_wipe: Some(wipe_request(paths.data_dir().to_path_buf())),
        debug_logs: Some(Box::new(move |enabled| debug_logs.set_enabled(enabled))),
    };
    run_desktop_app_with(services, extras)
        .map_err(|error| anyhow::anyhow!("window failed: {error}"))
}

/// "Delete all saved data": marks the folder; the next start wipes it before opening anything.
fn wipe_request(data_dir: std::path::PathBuf) -> WipeRequest {
    Box::new(move || request_wipe(&data_dir))
}
