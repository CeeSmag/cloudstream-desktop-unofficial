//! Windows presentation under the foreground permission granted by CloudStream.
use tao::{platform::windows::WindowExtWindows, window::Window};
use winsafe::{co, prelude::*, HWND};

#[allow(unsafe_code)]
fn with_handle(window: &Window, operation: impl FnOnce(&HWND)) {
    // SAFETY: Tao owns this live HWND on the event-loop thread throughout this call.
    // The wrapper neither destroys it nor escapes this function.
    let handle = unsafe { HWND::from_ptr(window.hwnd() as *mut _) };
    operation(&handle);
}

pub(super) fn show(window: &Window) {
    with_handle(window, |handle| {
        handle.ShowWindow(co::SW::SHOWNOACTIVATE);
    });
}

pub(super) fn hide(window: &Window) {
    with_handle(window, |handle| {
        handle.ShowWindow(co::SW::HIDE);
    });
}

pub(super) fn activate(window: &Window) {
    with_handle(window, |handle| {
        if handle.IsWindowVisible() {
            let activated = handle.SetForegroundWindow();
            tracing::debug!(activated, "verification foreground request");
        }
    });
}
