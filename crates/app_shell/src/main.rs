//! The `cloudstream` executable.
#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_shell::{
    build_services, build_webview, request_wipe, spawn_wipe_helper, start_cloudflare_watch,
    start_logging, start_storage_cleanup, validate_wipe_target, wipe_if_requested, AppPaths,
    RetryPlan, ServiceOptions,
};
use ui_slint::{run_desktop_app_with, AppExtras, WipeRequest};

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::from_environment()?;
    // First of all: a fallback start-up wipe runs before anything is opened.
    let wipe = wipe_if_requested(paths.data_dir(), RetryPlan::default());
    let debug_logs = start_logging(&paths.log_file());
    wipe.log();
    tracing::info!(data_dir = %paths.data_dir().display(), "cloudstream starting");
    let services = build_services(&paths, &ServiceOptions::default())?;
    let cleanup = start_storage_cleanup(paths.data_dir(), &services, &debug_logs);
    let wipe_on_exit = Arc::new(AtomicBool::new(false));
    // Verification windows open only for sites the viewer allowed (Verify in the app).
    let cloudflare = Some(start_cloudflare_watch(paths.data_dir(), &services.settings));
    let extras = AppExtras {
        cloudflare,
        data_dir: Some(paths.data_dir().to_path_buf()),
        request_wipe: Some(wipe_request(
            paths.data_dir().to_path_buf(),
            Arc::clone(&wipe_on_exit),
        )),
        debug_logs: Some(Box::new(move |enabled| debug_logs.set_enabled(enabled))),
        cleanup: Some(cleanup.handle()),
        webview: Some(build_webview(paths.data_dir())),
    };
    let result = run_desktop_app_with(services, extras)
        .map_err(|error| anyhow::anyhow!("window failed: {error}"));
    // Stop the background cleanup worker before checking for a post-exit data wipe.
    drop(cleanup);
    // Services, sidecar JVMs, SQLite, and cloudstream-verify are dropped now; finish the wipe.
    if wipe_on_exit.load(Ordering::SeqCst) {
        run_post_exit_wipe(paths.data_dir());
    }
    result
}

/// "Delete all saved data": validates the folder, writes the fallback marker, and signals exit wipe.
fn wipe_request(data_dir: PathBuf, wipe_on_exit: Arc<AtomicBool>) -> WipeRequest {
    Box::new(move || {
        let portable_exe = std::env::var_os("CS_PORTABLE_EXE").map(PathBuf::from);
        let current_exe = std::env::current_exe().ok();
        validate_wipe_target(&data_dir, portable_exe.as_deref(), current_exe.as_deref())?;
        request_wipe(&data_dir)?;
        wipe_on_exit.store(true, Ordering::SeqCst);
        Ok(())
    })
}

/// Removes the Windows AppContainer profile and starts the detached `%TEMP%` helper so `data_dir`
/// (including unpacked portable `runtime-*`) is deleted completely once this process exits.
fn run_post_exit_wipe(data_dir: &Path) {
    if let Err(error) = extension_runtime::delete_sidecar_app_container() {
        tracing::warn!(%error, "could not remove sidecar AppContainer profile");
    }
    if let Err(error) = spawn_wipe_helper(data_dir) {
        tracing::error!(
            data_dir = %data_dir.display(),
            %error,
            "could not start post-exit data wipe helper; next launch will wipe via marker"
        );
    }
}
