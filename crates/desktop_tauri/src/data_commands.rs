//! The "Delete all saved data" command of the General settings page.

use std::path::PathBuf;
use std::time::Duration;

use app_shell::request_wipe;
use tauri::{AppHandle, State};

use crate::app_state::AppState;
use crate::source_manager::SourceManager;

/// Where the app keeps everything it saves (database, extensions, saved sources, subtitles, logs).
pub struct DataFolder(pub PathBuf);

/// Gives the page time to receive the answer and clear its own saved state before the app restarts.
const RESTART_DELAY: Duration = Duration::from_millis(600);

/// Asks for all saved data to be deleted, then restarts the app. The deletion itself runs at the
/// next start, before anything is opened (see `app_shell::wipe_if_requested`).
#[tauri::command]
#[tracing::instrument(level = "info", skip_all, err)]
pub async fn delete_all_data(
    app: AppHandle,
    folder: State<'_, DataFolder>,
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    sources.close();
    stop_playback(&state);
    request_wipe(&folder.0)
        .map_err(|error| format!("Cannot ask for the saved data to be deleted: {error}"))?;
    tracing::info!("all saved data will be deleted at the next start; restarting");
    schedule_restart(app);
    Ok(())
}

/// Stops the video, so nothing writes progress while the data is about to go.
fn stop_playback(state: &AppState) {
    let services = state.read();
    if let Some(playback) = services.playback.as_ref() {
        if let Err(error) = playback.stop() {
            tracing::warn!(%error, "cannot stop the player before deleting the saved data");
        }
    }
}

fn schedule_restart(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("restart-after-wipe".to_owned())
        .spawn(move || {
            std::thread::sleep(RESTART_DELAY);
            restart(&app);
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot schedule the restart; close and reopen the app");
    }
}

/// Starts the app again and ends this process. The portable exe and the AppImage are started by their
/// own file (the program inside them is only a part of them); everything else restarts itself.
fn restart(app: &AppHandle) {
    let launcher = std::env::var_os("CS_PORTABLE_EXE").or_else(|| std::env::var_os("APPIMAGE"));
    if let Some(launcher) = launcher {
        match std::process::Command::new(&launcher).spawn() {
            Ok(_) => {
                app.exit(0);
                return;
            }
            Err(error) => tracing::warn!(%error, "cannot start the app again by its own file"),
        }
    }
    app.restart();
}
