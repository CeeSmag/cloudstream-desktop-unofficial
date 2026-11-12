//! Native video: mpv draws on the GPU into the main window, under the transparent web view.
//!
//! The window is transparent and so is the page's player screen; mpv is given the window's own
//! handle (`wid`) and makes a child window of its own below the web view, so the React controls
//! float over the picture. Frames never pass through the CPU or the IPC. Where this is not
//! possible (not Windows, or no window handle) the player keeps drawing into the page's canvas as
//! before. On Windows native is the only mode: there is no setting and no override. See `docs/NATIVE_VIDEO_PLAN.md`.

use tauri::{App, Manager, State};

/// Which picture path the player was started with; read by the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoMode {
    /// True when mpv draws into the main window itself.
    pub native: bool,
}

/// Whether to try native video: Windows only for now.
pub(crate) fn wants_native(windows: bool) -> bool {
    windows
}

/// The handle mpv should draw into, or `None` to keep the canvas player.
pub fn window_handle(app: &App) -> Option<i64> {
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

/// Tells the page whether the picture comes from mpv's own window (no canvas needed).
#[tauri::command]
pub async fn get_video_mode(mode: State<'_, VideoMode>) -> Result<bool, String> {
    Ok(mode.native)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_is_used_on_windows_only() {
        assert!(wants_native(true));
        assert!(!wants_native(false));
    }
}
