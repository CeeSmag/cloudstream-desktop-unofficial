//! Tauri commands for the transport controls: pause, play and seek.
//!
//! After a video has played to its end mpv has nothing loaded, so Play and a seek would do nothing.
//! There they start the same stream again (see `restart_if_ended`).

use tauri::State;

use crate::app_state::AppState;
use crate::source_commands::restart_if_ended;
use crate::source_manager::SourceManager;

/// The message when the player could not be reached.
const UNAVAILABLE: &str = "Playback service unavailable";

/// Pauses current playback.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn pause_playback(state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| UNAVAILABLE.to_string())?;
    playback.set_paused(true).map_err(|error| error.to_string())
}

/// Resumes current playback; after the end of the video, plays it again from the start.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn resume_playback(
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| UNAVAILABLE.to_string())?;
    if restart_if_ended(&sources, playback, 0)? {
        return Ok(());
    }
    playback
        .set_paused(false)
        .map_err(|error| error.to_string())
}

/// Seeks to a specific millisecond position; after the end of the video, plays it again from there.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(position_ms = ?position_ms), err)]
pub async fn seek_to(
    position_ms: i64,
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| UNAVAILABLE.to_string())?;
    if restart_if_ended(&sources, playback, position_ms)? {
        return Ok(());
    }
    playback
        .seek_to(position_ms)
        .map_err(|error| error.to_string())
}
