# Native video window — tasks

Plan and the facts behind it: [NATIVE_VIDEO_PLAN.md](NATIVE_VIDEO_PLAN.md) (F1–F11 are referenced below).
Mark a task `[x]` only when it is done **and** verified. Nothing is started yet.

## Phase 0 — spike (decision gate)

- [ ] 0.1 Get a local 2K test file and set `CS_BENCH_VIDEO`; record CPU use and dropped frames of the **current**
      canvas path on this PC as the baseline (`t4_responsiveness.rs` timings).
- [ ] 0.2 Spike B: child HWND for mpv (`wid`) under a transparent web view. Does video show? (F2 says it failed on
      the main HWND.) Record yes/no with a screenshot.
- [ ] 0.3 Spike A: child HWND for mpv plus a transparent, undecorated top-level Tauri window above it with one
      test button. Video visible? Button clickable? Window follows move/resize?
- [ ] 0.4 Repeat 0.2/0.3 with and without `--disable-gpu` in `additionalBrowserArgs` (F10).
- [ ] 0.5 Measure CPU use and dropped frames of the winning layout on the same 2K file.
- [ ] 0.6 **Gate:** write the result at the end of the plan. If neither layout works, stop and do Phase 4 only.

## Phase 1 — native video surface (only if the gate passes)

- [ ] 1.1 Decide where the Win32 code lives and add the `RULES.md` exception for `unsafe` (one module, `// SAFETY:`
      comments).
- [ ] 1.2 `NativeVideoSurface`: create, resize/move, show/hide, destroy a child HWND.
- [ ] 1.3 Player construction takes the surface's `wid` (F1: today `main.rs:178` passes `None`).
- [ ] 1.4 Failure path: if the surface or `vo=gpu` fails, fall back to the canvas player and log why.
- [ ] 1.5 Tests: surface lifecycle, fallback on failure.

## Phase 2 — controls on top

- [ ] 2.1 Page reports the player area rectangle (model on `place_cloudflare_panel`, async commands only — see
      `deadlock_tests.rs`).
- [ ] 2.2 Host the existing `PlayerOverlay.tsx` UI in the chosen layer; make its background transparent over the video.
- [ ] 2.3 Fullscreen (`set_window_fullscreen`), resize, minimize/restore, move, focus, always-on-top behaviour.
- [ ] 2.4 Keyboard shortcuts and clicks still reach the player (`playerShortcuts.ts`, `clickSorter.ts`).
- [ ] 2.5 Race tests for rectangle updates versus window events, with the proof-of-failure step (recipe 6).
- [ ] 2.6 Remove or hide `VideoCanvas` and the `get_video_frame` loop while the native surface is active.

## Phase 3 — mode switch

- [ ] 3.1 Setting `video_mode` (Auto / Native / Canvas) in `SettingsService`; applied at restart because the player is
      built once (F1).
- [ ] 3.2 Settings > Playback control and text.
- [ ] 3.3 Auto rule: native on Windows when the surface worked, canvas otherwise (F4: Linux Wayland, macOS).
- [ ] 3.4 Log which mode is active at start.

## Phase 4 — quality fixes (independent, can run in parallel)

- [ ] 4.1 "Max quality" setting; `playable_links_best_first` prefers links at or under the cap, keeps the rest as fallbacks.
- [ ] 4.2 Automatic step-down: on sustained dropped frames, switch to the next lower verified source at the same
      position (reuse `source_fallback.rs`).
- [ ] 4.3 Decoder options in `player_configuration.rs`: `vd-lavc-threads=0`, `vd-lavc-fast`, `framedrop=vo`, and try
      `hwdec=auto-safe`/`d3d11va` before copy mode. Measure each with `CS_BENCH_VIDEO`; keep only what helps.
- [ ] 4.4 Lower the 1280 px render cap on slow machines (adaptive).
- [ ] 4.5 Tests for the sort rule, the step-down trigger and its limits.

## Phase 5 — verify and finish

- [ ] 5.1 Run on an old laptop with a 2K stream; record before/after.
- [ ] 5.2 `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
      frontend `npm run build` and tests.
- [ ] 5.3 Update `AGENTS.md` (new modules, setting, commands) and the IPC command table.
- [ ] 5.4 Review the diff before merging to `main`.

## Open questions for the owner

1. Is Windows-only acceptable for the native mode (Linux and macOS stay on canvas)?
2. Why was `--disable-gpu` added to WebView2? Removing it may help or hurt.
3. Is a restart acceptable when the video mode setting changes?
