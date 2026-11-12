# Native video window — plan

Branch: `feat/native-video-window`. Status: **plan only, nothing built yet.**
Task list: [NATIVE_VIDEO_TASKS.md](NATIVE_VIDEO_TASKS.md).

## 1. Problem

Old laptops lag on 2K/4K streams. Today every frame takes this path:

1. mpv decodes (`hwdec=auto-copy-safe`: the GPU may decode, then each frame is **copied back** to RAM; with no
   usable GPU decoder the CPU decodes alone).
2. mpv's **software renderer** (`vo=libmpv`, `sw-fast`) scales it on the CPU to at most 1280 px wide
   (`frame_renderer.rs`, `MAX_RENDER_WIDTH`).
3. `to_rgba` converts it, `get_video_frame` copies it through the Tauri IPC (about 3.7 MB per 720p frame),
   and `VideoCanvas.tsx` paints it on a `<canvas>`.

Goal: let mpv decode **and draw on the GPU into a real OS window** (like VLC), with the app's controls
(seek bar, menus, skip button) still on top of the video.

## 2. Verified facts (checked in the code or online)

| # | Fact | Source |
|---|------|--------|
| F1 | The app picks the player mode once at start: `main.rs:176-178` passes `player_wid: None`, so `service_wiring.rs:93` sets `embedded: true`. A `wid` mode already exists in `player_configuration.rs` (`vo=gpu`, `hwdec=auto-safe`). | code |
| F2 | The first Tauri version tried exactly "transparent window over a native video window" and it **failed**: the transparent window showed the desktop, never the video, because "mpv attached to the main HWND is covered by WebView2" (commit `cd6ad46`). That attempt used the **main window's** HWND, not a separate child window. | git history |
| F3 | libmpv's own header says the software renderer is "extremely simple (but slow)... You probably don't want to use this", and that window embedding "can cause various issues, especially with GUI toolkits". The app uses the software renderer today. | [render.h](https://rust-digger.code-maven.com/src/libmpv-sys-3.1.0/include/render.h) |
| F4 | `--wid` embedding works on X11, Windows and macOS only; the render API is the route that works on Wayland. One third-party issue says so; current mpv docs not checked. | [libmpv client.h](https://git.tjdev.de/mirror/mpv/src/branch/master/libmpv/client.h), [Celluloid #900](https://www.github.com/celluloid-player/celluloid/issues/900) |
| F5 | WebView2 `DefaultBackgroundColor` can be fully transparent, "showing hosting app content". Only fully opaque or fully transparent values are allowed. This does **not** prove that a sibling native window behind it shows through. | [Microsoft docs](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2controller.defaultbackgroundcolor?view=webview2-dotnet-1.0.2957.106) |
| F6 | Microsoft's guidance for mixing native and web content ("airspace"): transparency works only with **entire top-level windows**, not child windows; the workaround is a separate top-level window positioned over the area. | [MS blog](https://learn.microsoft.com/en-us/archive/blogs/changov/webbrowser-control-on-transparent-wpf-window) |
| F7 | `tauri-plugin-libmpv` claims Windows embedding works with a transparent Tauri window plus a transparent page background (Linux embedding "not working"). Its README gives the recipe but does not explain how it avoids F2; treat as a lead, not proof. | [docs.rs](https://docs.rs/crate/tauri-plugin-libmpv/0.3.0/source/README.md) |
| F8 | The app already builds a second native browser view inside the main window (`cloudflare_panel.rs`), so Tauri's `unstable` multiwebview feature is enabled (`Cargo.toml`: `tauri = { features = ["unstable"] }`). | code, [Tauri docs](https://docs.rs/tauri/2.2.0/tauri/webview/struct.WebviewBuilder.html) |
| F9 | A transparent, undecorated, always-on-top Tauri window with `set_ignore_cursor_events` is a known pattern, but a click-through window cannot detect the mouse re-entering, and a fully click-through window also loses OS drag events. | [Tauri issue 9250](https://github.com/tauri-apps/tauri/issues/9250) |
| F10 | `tauri.conf.json` starts WebView2 with `--disable-gpu`, so the page is composited on the CPU. This may interact with transparency and with the cost of a second webview. Reason for the flag is not recorded. | code |
| F11 | Whole player UI is `PlayerOverlay.tsx` (295 lines) over `VideoCanvas.tsx` (80 lines), at `fixed inset-0 z-50 bg-black`. | code |

**Not verified:** that WebView2 transparency can reveal a native sibling window. F2 says it did not when mpv used
the main HWND. This is the one question the whole plan depends on, so the plan starts with a spike (Phase 0).

## 3. Candidate designs

- **A. Overlay window (recommended to try first).** Main window stays opaque. mpv draws into a dedicated native
  child window placed over the player area. The controls live in a second **top-level** transparent, undecorated
  Tauri window that follows the main window and sits on top (F6, F9). Matches Microsoft's own advice.
  Costs: keep two windows in sync (move, resize, minimize, fullscreen, focus), route keyboard and clicks.
- **B. Transparent web view over video child window.** The video sits in its own child HWND *under* a transparent
  web view. Cheapest to test, but F2 says this style failed once; retest only with a dedicated child HWND.
- **C. Keep the canvas, make it cheaper.** Quality cap, automatic step-down to a lower stream, better `hwdec`,
  smaller render size. Independent of A and B, works on every platform, and is the fallback in any case.
- **D. mpv render API with OpenGL/D3D into our own surface.** Avoids the CPU renderer but needs our own GL
  context and still ends in the web page unless combined with A or B. Not pursued now.

## 4. Plan

**Phase 0 — spike (Windows only, throw-away code).** Prove or kill A and B in a tiny test with a real mpv
playing a local 2K file. Decision gate: if neither shows video with a control on top, stop and ship only C.

**Phase 1 — native video surface.** A `NativeVideoSurface` (child HWND) in `desktop_tauri`, with all Win32 code in
one small module (project rule: `unsafe` is forbidden outside `player_mpv/src/mpv_api.rs`, so this module needs a
documented exception in `RULES.md`, or a tiny new crate). Player is created with that `wid`.

**Phase 2 — controls on top.** Chosen design (A or B) hosts the existing `PlayerOverlay` UI. The page reports the
player area rectangle (like `place_cloudflare_panel`) so the surface follows it. Fullscreen, resize, minimize and
focus handled and tested.

**Phase 3 — mode switch.** Because the player is built once at start (F1), the mode is a setting applied at
restart ("Hardware video mode": Auto / On / Off). Auto = native window on Windows when it works, canvas
otherwise. Canvas stays the fallback on Linux/Wayland/macOS (F4) and on any failure.

**Phase 4 — quality fixes (design C, can start in parallel).** Max-quality setting, automatic step-down to a
lower verified source on dropped frames, `hwdec` and decoder tuning, measured with `CS_BENCH_VIDEO`.

**Phase 5 — verify and release notes.** Test on an old laptop; update `AGENTS.md`.

## 5. Risks

- **R1** WebView2 will not show a native window through transparency (F2, F5). Mitigation: Phase 0 gate; A avoids it.
- **R2** Two-window sync bugs (focus, always-on-top over other apps, fullscreen). Mitigation: owner window
  relationship, dedicated tests, canvas fallback toggle.
- **R3** Mouse handling across layers (F9). Mitigation: overlay is not click-through over controls; clicks on
  empty video area handled by the overlay and forwarded as commands (the app already pauses by IPC).
- **R4** Concurrency in new shared state (rectangle updates racing with window events). Project rule requires
  race tests (`AGENTS.md` recipe 6).
- **R5** Linux Wayland and macOS unsupported by `wid` (F4): they stay on the canvas path.
- **R6** `--disable-gpu` (F10) may make a transparent overlay slow or glitchy; test with and without it.
- **R7** The project's own limits: files ≤ 300 lines, functions ≤ 40 lines, `unsafe` confined, no `unwrap`.

## 6. Done means

Windows, old laptop, 2K stream: smooth playback with controls visible and working; fullscreen works; seeking,
pause, subtitles, source switching unchanged; falling back to canvas works; all quality gates pass.
