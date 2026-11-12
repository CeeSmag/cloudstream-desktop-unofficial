# Native video window — plan

Branch: `feat/native-video-window`. Status: **Phase 0 mostly done. Chosen design: E (one window, mpv under a
transparent web view).** Not yet proven: input, resize/maximize/fullscreen, heavy-file CPU. Production code not started.

## 1. Problem

Old laptops lag on 2K/4K streams. Today every frame takes this path:

1. mpv decodes (`hwdec=auto-copy-safe`: the GPU may decode, then each frame is **copied back** to RAM; with no
   usable GPU decoder the CPU decodes alone).
2. mpv's **software renderer** (`vo=libmpv`, `sw-fast`) scales it on the CPU to at most 1280 px wide
   (`frame_renderer.rs`, `MAX_RENDER_WIDTH`).
3. `to_rgba` converts it, `get_video_frame` copies it through the Tauri IPC (about 3.7 MB per 720p frame),
   and `VideoCanvas.tsx` paints it on a `<canvas>`.

Goal: let mpv decode **and draw on the GPU into a real OS window** (like VLC), with the app's controls
(seek bar, menus, skip button) still on top of the video, in the **same window**.

## 2. Verified facts (checked in the code, online, or by spike)

| # | Fact | Source |
|---|------|--------|
| F1 | The app picks the player mode once at start: `main.rs:176-178` passes `player_wid: None`, so `service_wiring.rs:93` sets `embedded: true`. A `wid` mode already exists in `player_configuration.rs` (`vo=gpu`, `hwdec=auto-safe`). Services are built before a window handle exists, so building the player with a `wid` needs restructuring. | code |
| F2 | The first Tauri version tried "transparent window over mpv attached to the main HWND" and reported failure: the transparent window showed the desktop and never the video (commit `cd6ad46`). The reason was not recorded. Spike E (F13) now shows this layout working, so that failure had another cause that is **still unexplained**. | git history, spike |
| F3 | libmpv's own header says the software renderer is "extremely simple (but slow)... You probably don't want to use this", and that window embedding "can cause various issues, especially with GUI toolkits". The app uses the software renderer today. | [render.h](https://rust-digger.code-maven.com/src/libmpv-sys-3.1.0/include/render.h) |
| F4 | `--wid` embedding works on X11, Windows and macOS only; the render API is the route that works on Wayland. One third-party issue says so; current mpv docs not checked. Stock libmpv has no `--wid` on macOS (Frame Player patches it). | [libmpv client.h](https://git.tjdev.de/mirror/mpv/src/branch/master/libmpv/client.h), [Celluloid #900](https://www.github.com/celluloid-player/celluloid/issues/900), [frame-player README](https://github.com/risenxxx/frame-player) |
| F5 | WebView2 `DefaultBackgroundColor` can be fully transparent; only fully opaque or fully transparent values are allowed. | [Microsoft docs](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2controller.defaultbackgroundcolor?view=webview2-dotnet-1.0.2957.106) |
| F6 | Microsoft's guidance on "airspace": transparency works for whole top-level windows; the workaround is a separate top-level window over the area (design A). | [MS blog](https://learn.microsoft.com/en-us/archive/blogs/changov/webbrowser-control-on-transparent-wpf-window) |
| F7 | **Verified:** `tauri-plugin-libmpv` takes the Tauri window's native handle (`raw_window_handle` -> `wid`) and gives it to mpv; there is no separate child window of ours. Its example sets `"transparent": true` for the window. | [plugin source](https://github.com/nini22P/tauri-plugin-libmpv) (`src/desktop.rs`, `src/utils.rs`) |
| F8 | The app already builds a second native browser view inside the main window (`cloudflare_panel.rs`); Tauri's `unstable` feature is enabled. | code |
| F9 | A transparent, undecorated Tauri window with `set_ignore_cursor_events` is a known pattern (relevant only to design A). | [Tauri issue 9250](https://github.com/tauri-apps/tauri/issues/9250) |
| F10 | `tauri.conf.json` starts WebView2 with `--disable-gpu`; reason not recorded. The spike showed design E works **with and without** it. | code, spike |
| F11 | Whole player UI is `PlayerOverlay.tsx` (295 lines) over `VideoCanvas.tsx` (80 lines), at `fixed inset-0 z-50 bg-black`. | code |
| F12 | **Verified:** Frame Player (Tauri 2 + libmpv, shipped, Windows and macOS) uses exactly design E: "mpv renders into a native child view behind a transparent webview"; the window is deliberately transparent, so it keeps an opaque backdrop until mpv paints and an opaque window background colour underneath. Notes: "nothing painted means the desktop shows through"; "two compositors have to agree about geometry" (fullscreen, resize). | [architecture.md](https://github.com/risenxxx/frame-player/blob/main/docs/architecture.md), [window rules](https://github.com/risenxxx/frame-player/blob/main/docs/rules/window.md) |
| F13 | **Verified by spike (this PC):** design E works; see section 7. | spike `abba31d` |
| F14 | **Verified:** the reference CloudStream desktop client on disk (`player_bridge.cpp`) keeps the top-level window opaque and sets only the web view's `DefaultBackgroundColor` to transparent; mpv and WebView2 are siblings in one container and the container is raised with `SetWindowPos(HWND_TOP)`. Same idea, hand-written in C++. | local reference code |
| F15 | Gemini 3.8 Flash (High) was asked to search GitHub; its top recommendation is design E. All 8 repositories it named exist. Its detailed claims (maximize lag explanation, WebView2 TextureStream, DirectComposition limits in `wry`) were **not** individually verified and are not relied on here. | agy CLI output |

## 3. Candidate designs

- **E. One window, mpv under a transparent web view (chosen).** mpv gets the main window's handle as `wid` and
  creates its own child window; the transparent web view sits above it, so the React controls float over the
  video. One window, GPU decode and display, no pixel copies, no second window to keep in sync.
- **A. Owned overlay window (fallback).** mpv in a child window, controls in a second transparent top-level window
  that follows the main one. Proven to work in spike A, but the overlay leads the maximize animation and needs
  window-sync code. Keep only if E fails a gate.
- **B. Own `STATIC` child window under a transparent web view. Dropped.** Fails twice (GPU on and off): shows the
  desktop, no video. (Why this fails while E works is unexplained.)
- **C. Keep the canvas, make it cheaper.** Quality cap, automatic step-down, `hwdec`, smaller render size.
  Independent of E and A, works everywhere, and stays the fallback on Linux, macOS and on any failure.
- **D. mpv render API (OpenGL/D3D) into our own surface.** Needs our own GL context and native controls. Not pursued.
- **Also considered and not chosen:** swapping to a controls bar beside the video; DirectComposition visual hosting
  (needs our own WebView2 host, Windows-only, large).

## 4. Plan

**Phase 0 — spike (Windows only, throw-away).** Done for the visuals (section 7). Still to prove for E: real
clicks, resize, maximize, fullscreen, whether lowering mpv's window is needed, CPU on a heavy file.

**Phase 1 — enable design E in the app.**
- Window `transparent: true`; every page keeps an opaque background (the pages already paint their own) and the
  player screen shows an opaque backdrop until mpv paints (F12).
- Build the player **after** the main window exists (F1), with `wid` = the window's native handle; keep the
  existing embedded/canvas player as the fallback.
- Win32 code (z-order of mpv's window, if needed) in one small module with `// SAFETY:` comments and a documented
  `RULES.md` exception.
- mpv's window must be hidden or absent while not playing (no white or black rectangle over the pages).

**Phase 2 — controls.** `PlayerOverlay.tsx` becomes transparent over the video; `VideoCanvas` and the
`get_video_frame` loop are skipped in native mode. Fullscreen, resize, maximize, minimize/restore, shortcuts and
click handling re-tested.

**Phase 3 — mode switch.** Setting "Hardware video mode" (Auto / On / Off), applied at restart. Auto = native on
Windows when it works, canvas otherwise (Linux Wayland, macOS: F4).

**Phase 4 — quality fixes (design C, can start in parallel).** Max-quality setting, automatic step-down to a lower
verified source on dropped frames, decoder tuning measured with `CS_BENCH_VIDEO`.

**Phase 5 — verify and finish.** Old laptop test, quality gates, `AGENTS.md`.

## 5. Risks

- **R1** Design E failed once before for an unknown reason (F2). Mitigation: Phase 0 checks and fallback to A or C.
- **R2** A transparent top-level window means "nothing painted = desktop visible" (F12): flashes at start and stop,
  and pages must always paint. Mitigation: opaque backdrop and window background colour.
- **R3** Fullscreen, resize and maximize must keep mpv's window and the web view in agreement (F12). Not tested yet.
- **R4** Mouse and keyboard: the web view is on top and receives input; the player is commanded through IPC as today.
  Real clicks on the web view are expected to work; verify in the app.
- **R5** Player is built before a window exists today (F1). Needs restructuring and a lazy or deferred player.
- **R6** Concurrency in any new shared state needs race tests (`AGENTS.md` recipe 6).
- **R7** Linux Wayland and macOS: no `--wid` (F4). They stay on the canvas.
- **R8** The project's limits: files ≤ 300 lines, functions ≤ 40 lines, `unsafe` confined, no `unwrap`.

## 6. Done means

Windows, old laptop, 2K stream: smooth playback with controls visible and working; fullscreen works; seeking,
pause, subtitles, source switching unchanged; falling back to canvas works; all quality gates pass.

## 7. Phase 0 results (this PC, Windows 11, 16 cores; spike code in commits `44cbed6` and `abba31d`)

Screenshots were taken of the full window at its real size.

| Test | Result |
|------|--------|
| **A**: mpv in a child window, transparent owned top-level window above it | **Works.** Video plays; the HTML control draws over it; clicks work (user tested by hand); the overlay follows window drags and resizes; playback smooth. **Flaw:** on maximize the overlay jumps to the final size before the window's animation ends. |
| **B**: own `STATIC` child window for mpv under a transparent web view | **Fails** (GPU flag on and off): the window shows the desktop, no video. |
| **E**: mpv given the main window's own handle (plugin style), transparent window and web view | **Works** (GPU flag on and off): video fills the window and the red test control floats over it. The child windows of the main window, top to bottom, were `WRY_WEBVIEW` then `mpv` (same size). |
| CPU (design A, mostly static clip) | 0.19-0.27 CPU-seconds per 5 s (about 5% of one core). Not compared with the canvas path; this PC is not an old laptop. |

**Not yet tested for E:** real mouse clicks on the control; resize, maximize, fullscreen; minimize/restore; whether
the code that lowers mpv's window is needed (E was captured with it on; the z-order was already correct when
printed); a heavy 2K/4K file; the canvas baseline.

**Gate:** provisional pass for E, final after the untested items. If E fails any of them, fall back to A, which
passed; C is independent either way.

Gotchas found (production code must handle them):
1. The test exe needs the Windows manifest that `tauri_build` adds only to **bin** targets; an example crashed at start with `0xC0000139`.
2. WebView2 windows of one app sharing a profile must have **identical** `additionalBrowserArgs`, or building the second fails with `0x8007139F` (matters for design A only).
3. `tauri.conf.json` creates the `main` window itself; a code-built window needs another label.
4. Data URLs need the `webview-data-url` feature.
5. When an own child window's content goes away it paints **white** (design B/A); E uses mpv's own window.
6. Screen captures of the window must be DPI-aware or they are cropped.
