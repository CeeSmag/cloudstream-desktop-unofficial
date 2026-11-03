//! Tauri-based desktop application shell for CloudStream.
#![forbid(unsafe_code)]
#![allow(clippy::unreachable)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_state;
mod catalog_commands;
mod check_gate;
mod debug_log_commands;
mod dto;
mod extension_commands;
mod library_commands;
mod playback_commands;
mod repository_discovery;
mod settings_commands;
mod source_commands;
mod source_manager;
mod source_session;
mod startup;
mod video_commands;
mod window_commands;

use app_shell::{build_services, start_logging, AppPaths, DebugLogs, ServiceOptions};
use app_state::AppState;
use tauri::{Builder, Manager, Wry};

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::from_environment()?;
    let debug_logs = start_logging(&paths.log_file());
    tracing::info!(data_dir = %paths.data_dir().display(), "cloudstream-tauri starting");

    build_app(paths, debug_logs)
        .run(tauri::generate_context!())
        .map_err(|error| anyhow::anyhow!("tauri error: {error}"))
}

// The body is one flat list of command names; there is nothing to split.
#[allow(clippy::too_many_lines)]
fn build_app(paths: AppPaths, debug_logs: DebugLogs) -> Builder<Wry> {
    tauri::Builder::default()
        .setup(move |app| init_services(app, &paths, debug_logs))
        .invoke_handler(tauri::generate_handler![
            startup::get_startup_status,
            debug_log_commands::get_debug_logging,
            debug_log_commands::set_debug_logging,
            debug_log_commands::open_logs_folder,
            debug_log_commands::log_from_webapp,
            catalog_commands::get_available_providers,
            catalog_commands::get_home_sections,
            catalog_commands::search_media,
            catalog_commands::get_media_details,
            playback_commands::play_media,
            playback_commands::skip_source_check,
            playback_commands::pause_playback,
            playback_commands::resume_playback,
            playback_commands::seek_to,
            playback_commands::stop_playback,
            playback_commands::get_playback_status,
            playback_commands::select_media_track,
            playback_commands::set_volume,
            source_commands::get_playback_sources,
            source_commands::switch_source,
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
            extension_commands::get_installed_extensions,
            extension_commands::get_repository_plugins,
            extension_commands::install_plugin,
            extension_commands::uninstall_extension,
            extension_commands::set_extension_trusted,
            video_commands::set_video_surface,
            window_commands::set_window_fullscreen,
            video_commands::get_video_frame,
        ])
}

fn init_services(
    app: &mut tauri::App,
    paths: &AppPaths,
    debug_logs: DebugLogs,
) -> Result<(), Box<dyn std::error::Error>> {
    let options = ServiceOptions {
        headless_player: false,
        player_wid: None,
    };
    let services = build_services(paths, &options)?;
    let debug_on = debug_log_commands::restore_saved_choice(&services.settings, &debug_logs);
    app.manage(debug_logs);
    let extensions = services.extensions.clone();
    let repositories = services.repositories.clone();
    app.manage(AppState::new(services));
    app.manage(source_manager::SourceManager::new(
        app_services::StreamChecker::new(std::sync::Arc::new(extension_repo::UreqProbe::default())),
    ));
    app.manage(startup::StartupTracker::default());
    spawn_extension_loading(app.handle().clone(), extensions, repositories, debug_on);
    Ok(())
}

/// Loads installed extensions off the UI thread (the first load converts bytecode and is slow).
fn spawn_extension_loading(
    handle: tauri::AppHandle,
    extensions: app_services::ExtensionService,
    repositories: app_services::RepositoryService,
    debug_on: bool,
) {
    let spawned = std::thread::Builder::new()
        .name("load-extensions".to_owned())
        .spawn(move || {
            if debug_on {
                extensions.set_debug_logging(true);
            }
            startup::load_extensions(&handle, &extensions);
            // Some extensions add repositories on their own each time they load.
            repository_discovery::watch(handle, extensions, repositories);
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start extension loading");
    }
}
