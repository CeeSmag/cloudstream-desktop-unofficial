# Low-end PC retest checklist

Manual checks after launch → Home → player work on the Slint product (`app_shell` / `ui_slint`). Prefer a machine with ≤4-8 GB RAM and several installed plugins. Memory budgets and how to measure them: `docs/MEMORY.md` (`just memory`).

## Performance profile

- [ ] Settings > General > **Performance & memory** shows this PC's RAM and cores and suggests a profile (up to 6 GB Low-end, up to 12 GB Balanced, above High-end); **Auto** follows it.
- [ ] Picking **Low-end** shows "Using the Low-end preset"; changing any single option below switches the line to "Custom: N options changed"; **Reset to recommended** returns to Auto and clears them.
- [ ] A warning appears when the chosen limits could use more than half of the RAM; it never blocks a choice.

## Plugins / RAM

- [ ] With the Low-end preset, **Plugin processes** shows **1** (choices 1 / 2 / 3 / 4 / 6 / Unlimited); an install from before profiles had its automatic limit dropped once and follows the profile.
- [ ] With 3+ plugins installed and Resident workers = **1**, at any time (loading, switching provider, searching): at most **one** `java` / sidecar process, except briefly while a call on the older plugin is still running (it is never cut short; the excess is parked when that call ends).
- [ ] Switching provider once: the previous plugin process is stopped before the new one starts; the live count stays ≤ the chosen limit.
- [ ] **Verified browser windows kept** (Low-end: 2): after several sites were verified and about 30 seconds passed, only that many `WebKitWebProcess` processes remain under `cloudstream-verify`; a site whose window was closed is verified again when it is needed.
- [ ] **Close the verification helper when idle** (Low-end: 2 min): after a Cloudflare check and the idle time, `cloudstream-verify` and its browser processes are gone; the next check starts it again.
- [ ] Trusted plugin spawn logs plain / non-AppContainer (Windows) or non-Landlock (Linux); Job / rlimit still apply.
- [ ] Untrusted plugin still uses AppContainer / Landlock when `data.dir` is set.

## Home and pictures

- [ ] Startup screen: plugins load first; Home fetch starts only after `ready` (startup can wait up to ~20 s for the first Home paint, then drops to Home's own loading state).
- [ ] Each Home row shows its first **10** posters; expanding a shelf shows **20** per page with Next/Prev; collapsing releases the expanded pictures.
- [ ] Idle Home with 1 resident plugin stays under the budget in `docs/MEMORY.md` (process-tree PSS).

## Settings (General / Preferences)

- [ ] Settings > General > **Video cache**: **Buffer size** offers **64 MB**…**4 GB** (default by profile: 128 MB Low-end, 256 MB Balanced, 1 GB High-end); **Read-ahead share** offers 25 / 50 / 75 / 90 %.
- [ ] Changing **Buffer size** or **Read-ahead share** while a video plays applies the new limits without restart.
- [ ] **Video output**: **Software** (default) or **GPU (no picture copies)**; a change applies after a restart. GPU used about 79 % less app-process CPU than software on a 4K stream here (same memory) and was verified on Linux/Wayland only: check picture orientation, seek, pause and window resize on the target machine.
- [ ] Settings > General > **Storage limit** offers **1–10 GB** (default **2 GB**); **Clean up storage now** finishes without freezing the UI.
- [ ] Settings > Preferences > **Playback**: **Play HLS streams through a local helper** can be left on (default) or switched off for comparison on a weak CPU.

## Player

- [ ] The player (libmpv) starts when the first video plays, not at launch: idle Home does not hold it. Playing the first video may take a moment longer.
- [ ] Open a title: stream checks run one after another (`check_worker_count` is not used yet).
- [ ] Leave the player (Back): playback **stops** (Slint has no "keep video ready" away-hold). The decoder and demuxer cache are freed logically, but the process's resident memory may not shrink at once (the allocator keeps pages).
- [ ] Embedded Slint video path: `hwdec=auto-safe` after load (software decode fallback unchanged).
- [ ] Smoke: start → Home → play → leave → open another title, with the Low-end profile.

## Packaging

- [ ] Rebuild Linux AppImage when player or shell slices change: `scripts/build_release.sh`.
- [ ] Smoke the AppImage on the target low-end PC (start → Home → play → leave).

## Deferred (not required for this retest)

- Search, Browse and Library poster counts are not capped (Home is).
- Vertical virtualization of Home (shelves scrolled out of sight keep their 10 posters).
