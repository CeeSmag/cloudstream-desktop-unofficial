# Low-end PC retest checklist

Manual checks after the launch → Home → player optimization slices. Prefer a machine with ≤8 GB RAM and several installed plugins.

## Plugins / RAM

- [ ] Fresh install (or cleared `plugin_worker_limit`): Settings > General shows **Plugin processes (low-end)** defaulting to **1**.
- [ ] With 3+ plugins installed, after Home is idle: at most **one** `java` / sidecar process (worker limit 1).
- [ ] Switching provider once: a second JVM starts, the previous one is parked; live count stays ≤ limit.
- [ ] Trusted plugin spawn logs plain / non-AppContainer (Windows) or non-Landlock (Linux); Job / rlimit still apply.
- [ ] Untrusted plugin still uses AppContainer / Landlock when `data.dir` is set.

## Home

- [ ] Startup screen: no Home / provider fetch until `ready`.
- [ ] First Home shelf paints before remaining shelves fill in.
- [ ] Storage cleanup and nav update check wait ~45 s after ready (not competing with first paint).

## Player

- [ ] Open a title: fewer parallel stream checks on a low-core CPU (2…12 by parallelism).
- [ ] Preferences: **Keep video ready when leaving** off → leave player → no large away-held demux; opening again searches/plays fresh.
- [ ] Same switch on → leave → open same title resumes from memory.
- [ ] Settings offers **128 MB** video cache.
- [ ] Embedded/canvas path (non-Windows native): `hwdec=auto-safe` after load (software decode fallback unchanged).

## Packaging

- [ ] Rebuild Linux AppImage when player / WebView slices change: `scripts/build_release.sh`.
- [ ] Smoke the AppImage on the target low-end PC (start → Home → play → leave).

## Deferred (not in these slices)

- Defer `MpvPlayer::auto_detect` until first `play_media` (larger `service_wiring` change).
- Home shelf virtualization beyond `PosterCard` `loading="lazy"`.
