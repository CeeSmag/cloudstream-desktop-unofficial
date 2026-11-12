# Native video window — tasks

Plan and the facts behind it: [NATIVE_VIDEO_PLAN.md](NATIVE_VIDEO_PLAN.md) (F1–F15, R1–R8 are referenced below).
Chosen design: **E** (one window, mpv under a transparent web view). Mark a task `[x]` only when it is done
**and** verified.

## Phase 0 — spike (decision gate)

- [ ] 0.1 Get a local 2K/4K test file with real motion (the recordings on this PC are mostly static) and set
      `CS_BENCH_VIDEO`; record CPU use and dropped frames of the **current** canvas path as the baseline
      (`t4_responsiveness.rs`).
- [x] 0.2 Spike B (own child window under a transparent web view): fails.
- [x] 0.3 Spike A (owned overlay window): works; maximize animation flaw (kept as fallback).
- [x] 0.4 `--disable-gpu` on and off: no difference for A and E.
- [x] 0.5 Spike E (mpv gets the main window handle, transparent window and web view): video shows with the control
      floating over it (GPU flag on and off).
- [x] 0.6 (verified in the real app by the owner: play, pause by click and Space, maximize, resize, fullscreen, Back, smooth playback) Spike E, still to test by hand: real mouse click on the control; resize by edges; maximize; fullscreen;
      minimize/restore; keep playing smoothly through all of them.
- [ ] 0.7 Spike E: is lowering mpv's window needed? Run without it and print the z-order.
- [ ] 0.8 Measure CPU use and dropped frames of E on the heavy file from 0.1 and compare with the canvas baseline.
- [x] 0.9 **Gate: passed for E** (verified in the real app). **Gate:** write the final decision at the end of the plan (E passes, or fall back to A).
- [ ] 0.10 Find out why design F2 (commit `cd6ad46`) failed while E works, or record that it stays unexplained.

## Phase 1 — enable design E in the app

Implemented in this branch (`native_video.rs`, `tauri.conf.json`, `PlayerOverlay.tsx`). Verified: builds, clippy and
all tests pass, the app starts in native mode and the transparent window shows its pages normally. **Verified by the owner in the real app
(play, pause, maximize, resize, fullscreen, Back, no flash on load).

- [x] 1.1 Window `transparent: true` in `tauri.conf.json`; make sure every page paints an opaque background (R2) and the
      player screen has an opaque backdrop until mpv paints.
- [x] 1.2 Build the player after the main window exists, with `wid` = its native handle (F1, R5); keep the embedded
      canvas player as the fallback and for other platforms.
- [ ] 1.3 If lowering mpv's window is needed (0.7): one small Win32 module, `// SAFETY:` comments, `RULES.md`
      exception.
- [x] 1.4 mpv's window is hidden or absent while not playing; no stray rectangle over the pages.
- [ ] 1.5 Failure path: if the native player cannot start, fall back to the canvas player and log why.
- [ ] 1.6 Tests: fallback on failure; window/player creation order.

## Phase 2 — controls

- [x] 2.1 `PlayerOverlay.tsx` transparent over the video; skip `VideoCanvas` and the `get_video_frame` loop in
      native mode.
- [x] 2.2 Fullscreen (`set_window_fullscreen`), resize, maximize, minimize/restore, focus (R3).
- [x] 2.3 Keyboard shortcuts and clicks still work (`playerShortcuts.ts`, `clickSorter.ts`); the web view receives
      input and commands the player through IPC (R4).
- [ ] 2.4 Race tests for any new shared state, with the proof-of-failure step (R6, recipe 6).

## Phase 3 — mode switch

- [x] 3.1 (dropped by the owner: native is the only mode on Windows, no setting) Setting `video_mode` (Auto / Native / Canvas) in `SettingsService`; applied at restart.
- [ ] 3.2 Settings > Playback control and text.
- [ ] 3.3 Auto rule: native on Windows when it worked, canvas otherwise (R7: Linux Wayland, macOS).
- [ ] 3.4 Log which mode is active at start.

## Phase 4 — quality fixes (independent, can run in parallel)

- [x] 4.1 (replaced by the owner's request: a 1K / 2K / 4K resolution filter in the player's Sources menu, same style as the subtitle filter) "Max quality" setting; `playable_links_best_first` prefers links at or under the cap, keeps the rest as fallbacks.
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
2. Why was `--disable-gpu` added to WebView2? E works with and without it; removing it might help the page.
3. Is a restart acceptable when the video mode setting changes?
