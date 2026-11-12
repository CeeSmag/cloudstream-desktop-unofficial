//! THROW-AWAY SPIKE (docs/NATIVE_VIDEO_PLAN.md, Phase 0). Windows only. Not part of the app.
//!
//! Question: can mpv draw into a native child window while HTML controls show on top of it?
//!   mode `b`: transparent web view in the main window, video child window sent to the bottom.
//!   mode `a`: video child window on top, controls in a transparent top-level window above it.
//!
//! Run: `SPIKE_VIDEO=<file> CS_LIBMPV_PATH=<dll> cargo run -p desktop_tauri --bin native_video_spike -- a`
#![allow(
    unsafe_code,
    missing_docs,
    clippy::print_stderr,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::ffi::c_void;
use std::path::PathBuf;

use player_api::{PlaybackRequest, VideoPlayer};
use player_mpv::{MpvPlayer, PlayerOptions};
use tauri::{
    LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

type Hwnd = isize;

#[link(name = "user32")]
extern "system" {
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: isize,
        instance: isize,
        param: *const c_void,
    ) -> Hwnd;
    fn GetWindow(hwnd: Hwnd, cmd: u32) -> Hwnd;
    fn GetClassNameW(hwnd: Hwnd, buf: *mut u16, max: i32) -> i32;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut [i32; 4]) -> i32;
    fn FindWindowExW(parent: Hwnd, after: Hwnd, class: *const u16, title: *const u16) -> Hwnd;
    fn SetWindowPos(
        hwnd: Hwnd,
        after: Hwnd,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const HWND_TOP: Hwnd = 0;
const HWND_BOTTOM: Hwnd = 1;
const SWP_NOACTIVATE: u32 = 0x10;

/// Page with a see-through body and one button, like the player controls would be.
/// Must equal `additionalBrowserArgs` in tauri.conf.json: windows sharing a profile need identical args.
const ARGS: &str = "--disable-gpu --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

const PAGE: &str = "<html><body style='margin:0;background:transparent'>\
<div style='position:absolute;left:20px;bottom:20px;padding:14px 22px;background:crimson;color:white;\
font:bold 22px sans-serif;border-radius:10px' id=b onclick=\"this.textContent='CLICKED'\">CONTROL ON TOP</div>\
</body></html>";

fn url_of(file: &std::path::Path) -> tauri::Url {
    tauri::Url::from_file_path(file).unwrap()
}

/// WebView2 args; `SPIKE_GPU=1` leaves the GPU on (the app disables it, see tauri.conf.json).
fn args() -> String {
    if std::env::var_os("SPIKE_GPU").is_some() {
        String::new()
    } else {
        ARGS.to_owned()
    }
}

/// Video fills the window; the controls strip sits along its bottom edge. Physical pixels.
fn layout(main: &tauri::WebviewWindow, child: Hwnd, overlay: &tauri::WebviewWindow) {
    let (Ok(origin), Ok(size)) = (main.inner_position(), main.inner_size()) else {
        return;
    };
    let strip = 90;
    // SAFETY: `child` is a live child window of `main`.
    unsafe {
        SetWindowPos(child, HWND_TOP, 0, 0, size.width as i32, size.height as i32, SWP_NOACTIVATE);
    }
    let _ = overlay.set_position(PhysicalPosition::new(
        origin.x,
        origin.y + size.height as i32 - strip,
    ));
    let _ = overlay.set_size(PhysicalSize::new(size.width, strip as u32));
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn video_child(parent: Hwnd, rect: (i32, i32, i32, i32), after: Hwnd) -> Hwnd {
    let class = wide("STATIC");
    let name = wide("video");
    // SAFETY: plain Win32 calls with valid, NUL-terminated strings and a live parent window.
    unsafe {
        let child = CreateWindowExW(
            0,
            class.as_ptr(),
            name.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            rect.0,
            rect.1,
            rect.2,
            rect.3,
            parent,
            0,
            0,
            std::ptr::null(),
        );
        SetWindowPos(child, after, rect.0, rect.1, rect.2, rect.3, SWP_NOACTIVATE);
        child
    }
}

fn start_player(wid: Hwnd) -> Result<MpvPlayer, String> {
    let dll = std::env::var("CS_LIBMPV_PATH").map_err(|e| e.to_string())?;
    let video = std::env::var("SPIKE_VIDEO").map_err(|e| e.to_string())?;
    let options = PlayerOptions {
        wid: Some(wid as i64),
        ..PlayerOptions::default()
    };
    let player = MpvPlayer::new(&PathBuf::from(dll), &options).map_err(|e| e.to_string())?;
    let request = PlaybackRequest {
        url: video,
        title: "spike".into(),
        ..PlaybackRequest::default()
    };
    player.play(&request).map_err(|e| e.to_string())?;
    Ok(player)
}

/// mpv makes its own child window (class "mpv") inside the handle it was given; send it to the back.
fn lower_mpv_window(parent: Hwnd) {
    let class = wide("mpv");
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        // SAFETY: Win32 calls on a live parent; a missing child just returns 0.
        unsafe {
            let found = FindWindowExW(parent, 0, class.as_ptr(), std::ptr::null());
            if found != 0 {
                SetWindowPos(found, HWND_BOTTOM, 0, 0, 0, 0, 0x13);
            }
        }
    }
    // SAFETY: walks the live child list top to bottom (GW_CHILD = 5, GW_HWNDNEXT = 2).
    unsafe {
        let mut child = GetWindow(parent, 5);
        while child != 0 {
            let mut name = [0u16; 64];
            let len = GetClassNameW(child, name.as_mut_ptr(), 64) as usize;
            let mut rect = [0i32; 4];
            GetWindowRect(child, &mut rect);
            eprintln!("z-order child {child:#x} class {} rect {rect:?}", String::from_utf16_lossy(&name[..len]));
            child = GetWindow(child, 2);
        }
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "a".into());
    tauri::Builder::default()
        .setup(move |app| {
            let file = std::env::temp_dir().join("native_video_spike.html");
            std::fs::write(&file, PAGE)?;
            let url = WebviewUrl::External(url_of(&file));
            let transparent = mode == "b" || mode == "c";
            let main = WebviewWindowBuilder::new(app, "spike", url.clone())
                .title(format!("spike {mode}"))
                .additional_browser_args(&args())
                .data_directory(std::env::temp_dir().join("spike_profile"))
                .inner_size(1000.0, 640.0)
                .position(100.0, 100.0)
                .transparent(transparent)
                .build()?;
            if let Some(config_window) = app.get_webview_window("main") {
                config_window.destroy()?;
            }
            let hwnd = main.hwnd()?.0 as Hwnd;
            let after = if transparent { HWND_BOTTOM } else { HWND_TOP };
            let child = if mode == "c" { hwnd } else { video_child(hwnd, (0, 0, 640, 360), after) };
            eprintln!("spike {mode}: main hwnd {hwnd:#x}, video child {child:#x}");
            if mode == "a" {
                let overlay = WebviewWindowBuilder::new(app, "overlay", url)
                    .parent(&main)?
                    .additional_browser_args(&args())
                    .data_directory(std::env::temp_dir().join("spike_profile"))
                    .decorations(false)
                    .transparent(true)
                    .skip_taskbar(true)
                    .focused(false)
                    .build()?;
                layout(&main, child, &overlay);
                let (main_for_events, overlay_for_events) = (main.clone(), overlay.clone());
                main.on_window_event(move |event| {
                    if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
                        layout(&main_for_events, child, &overlay_for_events);
                    }
                });
            }
            let _ = (LogicalPosition::new(0, 0), LogicalSize::new(0, 0));
            if mode == "c" {
                std::thread::spawn(move || lower_mpv_window(hwnd));
            }
            match start_player(child) {
                Ok(player) => app.manage(player),
                Err(error) => {
                    eprintln!("spike: player failed: {error}");
                    false
                }
            };
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run spike");
}
