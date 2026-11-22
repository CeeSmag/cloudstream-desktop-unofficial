# Low-end PC retest checklist

Manual checks after launch → Home → player work on the Slint product (`app_shell` / `ui_slint`). Prefer a machine with ≤8 GB RAM and several installed plugins.

## Plugins / RAM

- [ ] Fresh install (or cleared `plugin_worker_limit`): Settings > General > **Plugin processes (low-end PC)** → **Resident plugin JVMs** defaults to **1** (choices: 1 / 2 / Unlimited). Existing installs that already had data migrate to Unlimited until chosen.
- [ ] With 3+ plugins installed and Resident workers = **1**, at any time (loading, switching provider, searching): at most **one** `java` / sidecar process, except briefly while a call on the older plugin is still running (it is never cut short; the excess is parked when that call ends).
- [ ] Switching provider once: a second JVM starts, the previous one is parked; live count stays ≤ the chosen limit.
- [ ] Trusted plugin spawn logs plain / non-AppContainer (Windows) or non-Landlock (Linux); Job / rlimit still apply.
- [ ] Untrusted plugin still uses AppContainer / Landlock when `data.dir` is set.

## Home

- [ ] Startup screen: plugins load first; Home fetch starts only after `ready` (startup can wait up to ~20 s for the first Home paint, then drops to Home's own loading state).
- [ ] With Resident workers = 1, first Home after cold start stays responsive while parked plugins are not woken until used.

## Settings (General / Preferences)

- [ ] Settings > General > **Video cache**: **Cache size** offers **128 MB**…**4 GB** (default **256 MB**); **Read ahead** offers 25 / 50 / 75 / 90 % (default **75 %**). On a low-end PC, set **Cache size** to **128 MB**.
- [ ] Changing **Cache size** or **Read ahead** while a video plays applies the new limits without restart.
- [ ] Settings > General > **Storage limit** → **Maximum cache & log storage** offers **1–10 GB** (default **2 GB**); **Clean up storage now** finishes without freezing the UI.
- [ ] Settings > Preferences > **Playback**: **Play HLS streams through a local helper** can be left on (default) or switched off for comparison on a weak CPU.

## Player

- [ ] Open a title: fewer parallel stream checks on a low-core CPU (`check_worker_count` clamps to 2…12 by `available_parallelism`).
- [ ] Leave the player (Back): playback **stops** (Slint has no “keep video ready” away-hold); demux/cache is released. Opening the same title again searches/plays fresh.
- [ ] Embedded Slint video path: `hwdec=auto-safe` after load (software decode fallback unchanged).
- [ ] Smoke: start → Home → play → leave → open another title, with Resident workers = 1 and Video cache = 128 MB.

## Packaging

- [ ] Rebuild Linux AppImage when player or shell slices change: `scripts/build_release.sh`.
- [ ] Smoke the AppImage on the target low-end PC (start → Home → play → leave).

## Deferred (not required for this retest)

- Defer `MpvPlayer::auto_detect` until first play (larger `service_wiring` change; still runs at startup today).
- Home shelf progressive paint / virtualization (Slint currently loads shelves then paints; posters hydrate in the background).
