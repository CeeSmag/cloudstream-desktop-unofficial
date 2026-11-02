//! Tauri-based desktop application shell for CloudStream.
#![forbid(unsafe_code)]
#![allow(clippy::unreachable)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_state;
mod catalog_commands;
mod dto;
mod library_commands;
mod playback_commands;
mod settings_commands;
mod window_mpv;

use app_shell::{build_services, start_logging, AppPaths, ServiceOptions};
use app_state::AppState;
use tauri::{Builder, Emitter, Manager, Wry};

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::from_environment()?;
    start_logging(&paths.log_file());
    tracing::info!(data_dir = %paths.data_dir().display(), "cloudstream-tauri starting");

    build_app(paths)
        .run(tauri::generate_context!())
        .map_err(|error| anyhow::anyhow!("tauri error: {error}"))
}

fn build_app(paths: AppPaths) -> Builder<Wry> {
    tauri::Builder::default()
        .setup(move |app| init_services(app, &paths))
        .invoke_handler(tauri::generate_handler![
            catalog_commands::get_available_providers,
            catalog_commands::get_home_sections,
            catalog_commands::search_media,
            catalog_commands::get_media_details,
            playback_commands::play_media,
            playback_commands::pause_playback,
            playback_commands::resume_playback,
            playback_commands::seek_to,
            playback_commands::stop_playback,
            playback_commands::get_playback_status,
            playback_commands::select_media_track,
            library_commands::get_bookmarks,
            library_commands::add_bookmark,
            library_commands::remove_bookmark,
            library_commands::is_bookmarked,
            library_commands::get_continue_watching,
            settings_commands::get_repositories,
            settings_commands::add_repository,
            settings_commands::remove_repository,
            settings_commands::get_disabled_providers,
            settings_commands::set_provider_enabled,
        ])
}

fn init_services(app: &mut tauri::App, paths: &AppPaths) -> Result<(), Box<dyn std::error::Error>> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| anyhow::anyhow!("main window not found"))?;
    let wid = window_mpv::extract_window_id(&window);
    let options = ServiceOptions {
        headless_player: false,
        player_wid: wid,
    };
    let services = build_services(paths, &options)?;
    let extensions = services.extensions.clone();
    app.manage(AppState::new(services));
    spawn_extension_loading(app.handle().clone(), extensions);
    Ok(())
}

/// Event the UI listens to so it can reload providers once extensions finish loading or change.
pub const PROVIDERS_CHANGED_EVENT: &str = "providers-changed";

/// Loads installed extensions off the UI thread (the first load converts bytecode and is slow).
fn spawn_extension_loading(handle: tauri::AppHandle, extensions: app_services::ExtensionService) {
    let spawned = std::thread::Builder::new()
        .name("load-extensions".to_owned())
        .spawn(move || {
            match extensions.load_installed() {
                Ok(report) => tracing::info!(
                    providers = report.providers.len(),
                    failures = report.failures.len(),
                    "installed extensions loaded"
                ),
                Err(error) => tracing::warn!(%error, "could not load installed extensions"),
            }
            if let Err(error) = handle.emit(PROVIDERS_CHANGED_EVENT, ()) {
                tracing::warn!(%error, "could not notify the UI about providers");
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start extension loading");
    }
}
