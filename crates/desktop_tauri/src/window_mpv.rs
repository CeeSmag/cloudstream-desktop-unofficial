//! Window handle extraction and libmpv hardware-accelerated overlay setup.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::WebviewWindow;

/// Extracts the native window handle as an integer ID for mpv `wid`.
pub fn extract_window_id(window: &WebviewWindow) -> Option<i64> {
    #[cfg(target_os = "windows")]
    {
        let handle = window.window_handle().ok()?;
        if let RawWindowHandle::Win32(win32) = handle.as_raw() {
            let hwnd_val = win32.hwnd.get() as i64;
            tracing::info!(hwnd = hwnd_val, "extracted Win32 HWND for mpv wid");
            return Some(hwnd_val);
        }
    }
    tracing::warn!("could not extract native window handle for mpv");
    None
}
