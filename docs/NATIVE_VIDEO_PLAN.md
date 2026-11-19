# Native video window — plan (ARCHIVE / REFERENCE ONLY)

**Not a product plan.** The shipping UI is `app_shell` + `ui_slint` with embedded mpv
(`vo=libmpv` frames into Slint). There is no WebView / Tauri chrome in the product build.

This document records the **former Tauri + React** investigation for Windows “native” video
(design E: mpv under a transparent web view). What shipped for that shell lives under
`archive/desktop_tauri/` (`native_video.rs`, transparent `tauri.conf.json`,
`PlayerOverlay` / `useNativeVideo`). Do not treat the phases below as open work for Slint.

Historical branch note: `feat/native-video-window`. Spike commits cited below: `44cbed6`, `abba31d`.

## 1. Problem (Tauri canvas path — historical)

On the old Tauri shell, every frame took roughly:

1. mpv decodes (`hwdec=auto-copy-safe`: GPU may decode, then each frame is **copied back** to RAM).
2. Software renderer (`vo=libmpv`, `sw-fast`) scales on the CPU (then capped, e.g. `MAX_RENDER_WIDTH`).
3. RGBA conversion, `get_video_frame` over Tauri IPC, paint on `VideoCanvas.tsx`.

Goal then: let mpv decode **and draw on the GPU into a real OS window**, with React controls
still on top in the **same window**.

## 2. Verified facts (checked in the code, online, or by spike)

| # | Fact | Source |
|---|------|--------|
| F1 | Tauri `main` once passed `player_wid: None`, so wiring used `embedded: true`. A `wid` mode already existed in `player_configuration.rs` (`vo=gpu`, `hwdec=auto-safe`). Services were built before a window handle existed, so a `wid` player needed deferred construction. | code (then) |
| F2 | An early Tauri attempt (“transparent window over mpv on the main HWND”) failed: desktop showed, no video (commit `cd6ad46`). Spike E (F13) later showed the layout working; that earlier failure’s cause was **not** recorded. | git history, spike |
| F3 | libmpv’s software renderer is documented as simple/slow; window embedding “can cause various issues, especially with GUI toolkits”. | [render.h](https://rust-digger.code-maven.com/src/libmpv-sys-3.1.0/include/render.h) |
| F4 | `--wid` embedding works on X11, Windows and (patched) macOS; Wayland generally needs the render API. Stock libmpv has no `--wid` on macOS (Frame Player patches it). | [libmpv client.h](https://git.tjdev.de/mirror/mpv/src/branch/master/libmpv/client.h), [Celluloid #900](https://www.github.com/celluloid-player/celluloid/issues/900), [frame-player README](https://github.com/risenxxx/frame-player) |
| F5 | WebView2 `DefaultBackgroundColor` can be fully transparent; only fully opaque or fully transparent values are allowed. | [Microsoft docs](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2controller.defaultbackgroundcolor?view=webview2-dotnet-1.0.2957.106) |
| F6 | Microsoft’s “airspace” guidance: transparency for whole top-level windows; separate top-level overlay is the classic workaround (design A). | [MS blog](https://learn.microsoft.com/en-us/archive/blogs/changov/webbrowser-control-on-transparent-wpf-window) |
| F7 | `tauri-plugin-libmpv` takes the Tauri window’s native handle as `wid`; example uses `"transparent": true`. | [plugin source](https://github.com/nini22P/tauri-plugin-libmpv) |
| F8 | The Tauri app already built a second native browser view inside the main window (`cloudflare_panel.rs`). | code (archive) |
| F9 | Transparent undecorated Tauri window + `set_ignore_cursor_events` is a known pattern (design A). | [Tauri issue 9250](https://github.com/tauri-apps/tauri/issues/9250) |
| F10 | `tauri.conf.json` started WebView2 with `--disable-gpu` on Windows; spike E worked with and without it. | code, spike |
| F11 | Player UI was `PlayerOverlay.tsx` over `VideoCanvas.tsx`. | archive frontend |
| F12 | Frame Player (Tauri 2 + libmpv) uses design E: mpv behind a transparent webview; nothing painted shows the desktop; geometry must stay in sync (fullscreen, resize). | [architecture.md](https://github.com/risenxxx/frame-player/blob/main/docs/architecture.md) |
| F13 | Spike on Windows 11: design E works; see section 7. | spike `abba31d` |
| F14 | Reference CloudStream desktop client (`player_bridge.cpp`): opaque top-level window, transparent web-view background only; mpv and WebView2 as siblings. | local reference code |
| F15 | External repo survey preferred design E; detailed secondary claims were **not** individually verified. | agy CLI output |

## 3. Candidate designs (historical choice)

- **E. One window, mpv under a transparent web view (chosen for Tauri).** mpv gets the main window
  handle as `wid`; transparent web view sits above; React controls float over the video.
- **A. Owned overlay window (fallback).** Proven in spike A; overlay can lead maximize animation.
- **B. Own `STATIC` child under transparent web view. Dropped.** Showed desktop, no video.
- **C. Cheaper canvas.** Independent fallback (also the non-Windows path in the archived shell).
- **D. mpv render API into our own surface.** Not pursued for Tauri.

## 4. Historical plan (do not execute for Slint)

Phases 0–5 below were the Tauri rollout checklist. Native mode later shipped on Windows in
`archive/desktop_tauri` (no Auto/On/Off setting there: native when a window handle exists).
Slint product work does **not** follow this checklist.

**Phase 0 — spike (Windows).** Visuals done (section 7). Remaining spike items were input,
resize/maximize/fullscreen, heavy-file CPU.

**Phase 1 — enable E.** Transparent window; deferred player with `wid`; canvas as fallback;
Win32 z-order if needed; hide mpv’s child while not playing.

**Phase 2 — controls.** Transparent overlay; skip canvas / `get_video_frame` in native mode.

**Phase 3 — mode switch.** Setting “Hardware video mode” was planned; archived Windows path
made native the only mode instead.

**Phase 4 — quality (design C).** Cap / step-down / decoder tuning (`CS_BENCH_VIDEO`).

**Phase 5 — verify.** Old laptop, gates, docs.

## 5. Risks (still relevant if revisiting Tauri archive)

- **R1** Design E failed once before for an unknown reason (F2).
- **R2** Transparent top-level window: nothing painted = desktop visible.
- **R3** Fullscreen / resize / maximize: mpv child and web view must agree on geometry.
- **R4** Web view on top receives input; player commanded through IPC.
- **R5** Player was built before a window existed → deferred construction.
- **R6** Shared state needs race tests.
- **R7** Linux Wayland / macOS: no stock `--wid` path → canvas (or render API).
- **R8** Project limits: file/function size, confined `unsafe`, no unwrap.

## 6. Done means (Tauri target — historical)

Windows, old laptop, 2K stream: smooth playback with controls; fullscreen; seek/pause/subs/source
switch unchanged; canvas fallback; quality gates pass.

## 7. Phase 0 results (Windows 11 spike; commits `44cbed6`, `abba31d`)

| Test | Result |
|------|--------|
| **A**: mpv child + transparent owned overlay | **Works.** Overlay can jump ahead of maximize animation. |
| **B**: own `STATIC` under transparent web view | **Fails** (GPU flag on/off): desktop, no video. |
| **E**: main-window `wid`, transparent window/web view | **Works** (GPU flag on/off). Child order: `WRY_WEBVIEW` then `mpv`. |
| CPU (design A, light clip) | ~5% of one core / 5 s on that PC; not a canvas baseline. |

**Not tested in the spike for E:** real control clicks; resize/maximize/fullscreen; minimize/restore;
whether lowering mpv’s window is required; heavy 2K/4K; canvas baseline.

Gotchas found then:
1. Test exe needs the Windows manifest `tauri_build` adds to **bin** targets.
2. WebView2 windows sharing a profile need identical `additionalBrowserArgs`.
3. Config-created `main` window vs code-built windows need distinct labels.
4. Data URLs need the `webview-data-url` feature.
5. Own child windows paint **white** when empty; E uses mpv’s own window.
6. Screen captures must be DPI-aware or they crop.
