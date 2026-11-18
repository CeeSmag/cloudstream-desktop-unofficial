//! Tauri commands that change the application window.

use tauri::Window;

/// Puts the window into or out of fullscreen. This is the native window's fullscreen, not the
/// browser's: the browser's fullscreen inside the webview can leave the mouse stuck.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(fullscreen = ?fullscreen), err)]
pub async fn set_window_fullscreen(fullscreen: bool, window: Window) -> Result<(), String> {
    tracing::info!(fullscreen, "changing the window's fullscreen state");
    window
        .set_fullscreen(fullscreen)
        .map_err(|error| format!("Cannot change fullscreen: {error}"))
}
