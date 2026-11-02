//! Wiring only: builds the real implementations and starts the app.
#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_paths;
mod logging_setup;
mod service_wiring;
mod sidecar_discovery;
mod unavailable_gateway;

fn main() -> anyhow::Result<()> {
    let paths = app_paths::AppPaths::from_environment()?;
    logging_setup::start(&paths.log_file());
    tracing::info!(data_dir = %paths.data_dir().display(), "cloudstream starting");
    let services = service_wiring::build_services(&paths)?;
    ui_app::run_desktop_app(services).map_err(|error| anyhow::anyhow!("window failed: {error}"))
}
