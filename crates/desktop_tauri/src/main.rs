//! Tauri-based desktop application shell for CloudStream.
#![forbid(unsafe_code)]
#![allow(clippy::unreachable)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_state;
mod browse_commands;
mod browse_genres;
mod catalog_commands;
mod check_gate;
mod check_gate_state;
mod check_progress;
mod data_commands;
mod debug_log_commands;
mod dto;
mod extension_commands;
mod failure_tally;
mod library_commands;
mod link_ledger;
#[cfg(test)]
mod opening_flow_tests;
mod playback_commands;
mod plugin_catalog_commands;
mod preferences_commands;
mod provider_switch;
#[cfg(test)]
mod race_tests;
#[cfg(test)]
mod recording_player;
mod repository_discovery;
mod saved_start;
#[cfg(test)]
mod session_race_tests;
mod settings_commands;
mod skip_commands;
mod source_commands;
mod source_discovery;
mod source_fallback;
mod source_fallback_steps;
mod source_manager;
mod source_notes;
mod source_session;
mod sources_cleanup;
mod startup;
mod subtitle_delivery;
#[cfg(test)]
mod subtitle_tests;
#[cfg(test)]
mod test_subtitles;
#[cfg(test)]
mod test_titles;
mod update_commands;
mod video_commands;
mod video_opening;
mod window_commands;

use app_shell::{
    build_services, start_logging, wipe_if_requested, AppPaths, DebugLogs, RetryPlan,
    ServiceOptions,
};
use app_state::AppState;
use tauri::{Builder, Manager, Wry};

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::from_environment()?;
    // First of all: a requested "delete all saved data" runs before anything is opened.
    let wipe = wipe_if_requested(paths.data_dir(), RetryPlan::default());
    let debug_logs = start_logging(&paths.log_file());
    wipe.log();
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
            preferences_commands::get_preferences,
            preferences_commands::set_preferred_audio,
            preferences_commands::set_preferred_subtitles,
            preferences_commands::set_auto_subtitles,
            preferences_commands::set_playback_option,
            provider_switch::provider_switched,
            skip_commands::get_skip_segments,
            debug_log_commands::get_debug_logging,
            debug_log_commands::set_debug_logging,
            debug_log_commands::open_logs_folder,
            debug_log_commands::log_from_webapp,
            catalog_commands::get_available_providers,
            catalog_commands::get_home_sections,
            catalog_commands::search_media,
            catalog_commands::get_media_details,
            browse_commands::browse_catalog,
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
            library_commands::get_resume_pointer,
            library_commands::get_show_progress,
            library_commands::remove_continue_watching,
            library_commands::clear_continue_watching,
            settings_commands::get_repositories,
            settings_commands::add_repository,
            settings_commands::remove_repository,
            settings_commands::get_disabled_providers,
            settings_commands::set_provider_enabled,
            extension_commands::get_installed_extensions,
            extension_commands::get_repository_plugins,
            extension_commands::refresh_repository_icons,
            plugin_catalog_commands::prepare_plugin_catalog,
            plugin_catalog_commands::list_all_plugins,
            extension_commands::install_plugin,
            extension_commands::uninstall_extension,
            extension_commands::set_extension_trusted,
            video_commands::set_video_surface,
            window_commands::set_window_fullscreen,
            video_commands::get_video_frame,
            update_commands::check_for_updates,
            update_commands::apply_update,
            data_commands::delete_all_data,
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
    app.manage(data_commands::DataFolder(paths.data_dir().to_path_buf()));
    preferences_commands::apply_saved(&services.settings, services.playback.as_ref());
    if let (true, Some(playback)) = (debug_on, services.playback.as_ref()) {
        playback.set_verbose_player_log(true);
    }
    let extensions = services.extensions.clone();
    let repositories = services.repositories.clone();
    let services_valid_from = services
        .settings
        .sources_valid_from()
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "cannot read the time of the last provider switch");
            0
        });
    app.manage(AppState::new(services));
    app.manage(source_manager::SourceManager::new(
        app_services::StreamChecker::new(std::sync::Arc::new(extension_repo::UreqProbe::default())),
        subtitle_loader(paths),
        sources_cleanup::open_title_cache(paths.data_dir(), services_valid_from),
    ));
    app.manage(std::sync::Arc::new(skip_times::SkipLookup::new(
        std::sync::Arc::new(skip_times::UreqSkipFetcher::default()),
    )));
    app.manage(std::sync::Arc::new(browse_commands::BrowseClient::default()));
    app.manage(startup::StartupTracker::default());
    spawn_extension_loading(app.handle().clone(), extensions, repositories, debug_on);
    Ok(())
}

/// How long downloaded subtitle files and the per-title lists are kept.
const SUBTITLE_KEEP: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

/// The loader that downloads, cleans and keeps subtitle files in the data folder; old ones are
/// removed in the background.
fn subtitle_loader(paths: &AppPaths) -> std::sync::Arc<subtitle_store::SubtitleLoader> {
    let cache =
        subtitle_store::SubtitleCache::new(paths.data_dir().join("subtitles"), SUBTITLE_KEEP);
    let loader = std::sync::Arc::new(subtitle_store::SubtitleLoader::new(
        cache,
        std::sync::Arc::new(subtitle_store::UreqSubtitleFetcher::default()),
        subtitle_store::LoadOptions::default(),
    ));
    let cleaner = std::sync::Arc::clone(&loader);
    let spawned = std::thread::Builder::new()
        .name("subtitle-cleanup".to_owned())
        .spawn(move || {
            let removed = cleaner.cache().prune();
            tracing::info!(removed, "old subtitle files removed");
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot start the subtitle cleanup");
    }
    loader
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
