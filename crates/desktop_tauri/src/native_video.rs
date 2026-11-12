//! Native video: mpv draws on the GPU into the main window, under the transparent web view.
//!
//! The window is transparent and so is the page's player screen; mpv is given the window's own
//! handle (`wid`) and makes a child window of its own below the web view, so the React controls
//! float over the picture. Frames never pass through the CPU or the IPC. Where this is not
//! possible (not Windows, a failed handle, or `CS_VIDEO_MODE=canvas`) the player keeps drawing
//! into the page's canvas as before. See `docs/NATIVE_VIDEO_PLAN.md`.

use tauri::{App, Manager, State};

/// The environment variable that forces a mode: `canvas` or `native`.
pub const MODE_VARIABLE: &str = "CS_VIDEO_MODE";

/// Which picture path the player was started with; read by the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoMode {
    /// True when mpv draws into the main window itself.
    pub native: bool,
}

/// Whether to try native video: Windows only, unless the variable says `canvas`.
pub(crate) fn wants_native(variable: Option<&str>, windows: bool) -> bool {
    match variable.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("canvas") => false,
        Some(value) if value.eq_ignore_ascii_case("native") => true,
        _ => windows,
    }
}

/// The handle mpv should draw into, or `None` to keep the canvas player.
pub fn window_handle(app: &App) -> Option<i64> {
    let variable = std::env::var(MODE_VARIABLE).ok();
    if !wants_native(variable.as_deref(), cfg!(windows)) {
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
    fn windows_uses_native_unless_told_otherwise() {
        assert!(wants_native(None, true));
        assert!(!wants_native(None, false));
        assert!(!wants_native(Some("canvas"), true));
        assert!(wants_native(Some(" NATIVE "), false));
        assert!(wants_native(Some("something else"), true));
    }
}
