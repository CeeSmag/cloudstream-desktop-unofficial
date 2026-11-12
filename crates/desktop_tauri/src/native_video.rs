//! Native video: mpv draws on the GPU into the main window, under the transparent web view.
//!
//! The window is transparent and so is the page's player screen; mpv is given the window's own
//! handle (`wid`) and makes a child window of its own below the web view, so the React controls
//! float over the picture. Frames never pass through the CPU or the IPC. Where this is not
//! possible (not Windows, or no window handle) the player keeps drawing into the page's canvas as
//! before. On Windows native is the only mode: there is no setting and no override. See `docs/NATIVE_VIDEO_PLAN.md`.

use serde::Serialize;
use tauri::{App, Manager, State};

/// Which picture path the player was started with; read by the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoMode {
    /// mpv draws into the main window itself, under the transparent web view.
    Native,
    /// mpv hands frames to the page, which paints them on a canvas.
    Canvas,
}

impl VideoMode {
    /// Native exactly when the player was given a window handle.
    pub fn from_handle(handle: Option<i64>) -> Self {
        if handle.is_some() {
            Self::Native
        } else {
            Self::Canvas
        }
    }
}

/// Whether to try native video: Windows only for now.
pub(crate) fn wants_native(windows: bool) -> bool {
    windows
}

/// Decides the video mode at start-up, stores it for the page (`get_video_mode`) and returns the
/// window handle mpv should draw into, or `None` to keep the canvas player.
pub fn prepare(app: &App) -> Option<i64> {
    let handle = window_handle(app);
    app.manage(VideoMode::from_handle(handle));
    handle
}

/// The handle mpv should draw into, or `None` to keep the canvas player.
fn window_handle(app: &App) -> Option<i64> {
    if !wants_native(cfg!(windows)) {
        tracing::info!("video mode: canvas");
        return None;
    }
    let window = app.get_webview_window("main")?;
    match window.hwnd() {
        Ok(handle) => {
            tracing::info!(
                hwnd = handle.0 as i64,
                "video mode: native, mpv draws into the main window"
            );
            Some(handle.0 as i64)
        }
        Err(error) => {
            tracing::warn!(%error, "no window handle; video mode: canvas");
            None
        }
    }
}

/// Tells the page where the picture comes from: mpv's own window (no canvas needed) or frames.
#[tauri::command]
pub async fn get_video_mode(mode: State<'_, VideoMode>) -> Result<VideoMode, String> {
    Ok(*mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_is_used_on_windows_only() {
        assert!(wants_native(true));
        assert!(!wants_native(false));
    }

    #[test]
    fn the_mode_is_native_exactly_when_there_is_a_window_handle() {
        assert_eq!(VideoMode::from_handle(Some(42)), VideoMode::Native);
        assert_eq!(VideoMode::from_handle(None), VideoMode::Canvas);
    }

    #[test]
    fn the_page_receives_the_mode_as_a_lowercase_word() {
        assert_eq!(
            serde_json::to_string(&VideoMode::Native).ok().as_deref(),
            Some("\"native\"")
        );
        assert_eq!(
            serde_json::to_string(&VideoMode::Canvas).ok().as_deref(),
            Some("\"canvas\"")
        );
    }
}
