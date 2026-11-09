//! The "Delete all saved data" command of the General settings page.

use std::path::PathBuf;
use std::time::Duration;

use app_shell::request_wipe;
use tauri::{AppHandle, State};

use crate::app_state::AppState;
use crate::data_helper;
use crate::source_manager::SourceManager;

/// Where the app keeps everything it saves (database, extensions, saved sources, subtitles, logs).
pub struct DataFolder(pub PathBuf);

/// Gives the page time to receive the answer and clear its own saved state before the app closes.
const EXIT_DELAY: Duration = Duration::from_millis(600);

/// Deletes all saved data and starts the app again. The deleting is done by a separate helper
/// process after this app has closed (nothing can be in use then); if the helper cannot be
/// started, a marker is left instead and the next start deletes everything (`app_shell::data_wipe`).
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
    match data_helper::start_helper(&folder.0) {
        Ok(()) => tracing::info!("helper started; closing so it can delete all saved data"),
        Err(error) => {
            tracing::warn!(%error, "cannot start the helper; the next start will delete instead");
            request_wipe(&folder.0)
                .map_err(|error| format!("Cannot delete the saved data: {error}"))?;
        }
    }
    schedule_exit(app);
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

/// Closes the app shortly, once the page has its answer. The helper (or, failing that, the user)
/// starts it again.
fn schedule_exit(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("close-after-delete".to_owned())
        .spawn(move || {
            std::thread::sleep(EXIT_DELAY);
            app.exit(0);
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot schedule closing the app; close and reopen it");
    }
}
