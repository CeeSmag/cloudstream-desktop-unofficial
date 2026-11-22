# Memory budget and measurements

Target: a **4 GB PC** with the whole app (every process it starts) at or under the budgets below.
Why a tree: the app is `cloudstream` + one `java` sidecar per resident plugin + the Cloudflare helper
and its WebKit/WebView2 children. One process alone hides most of the cost.

## Budgets (acceptance, total process-tree PSS)

| Scenario | Budget |
|---|---|
| S1 idle Home, 1 resident plugin | <= 450 MiB |
| S3 playing 1080p HLS | <= 700 MiB |

Other scenarios are tracked but have no budget yet; set one once a baseline exists.

## How to measure

```bash
# Follow a running app (or start it) and report the peak of the whole tree.
python3 scripts/measure_memory.py --launch --label S1 --duration 120 -- ./cloudstream
python3 scripts/measure_memory.py --pid <pid> --label S3 --duration 600 --max-total-mib 700
just memory --pid <pid> --label S1          # same script through just
```

Linux reports PSS (shared pages counted once, `/proc/<pid>/smaps_rollup`). Windows reports working
set, an upper bound. Do not compare the two OS numbers with each other.

Use a **scratch data directory** (`CS_DATA_DIR=$(mktemp -d)`), never the real one: measuring must not
touch the viewer's database, plugins or history.

For the JVM breakdown, run a measurement build with `-XX:NativeMemoryTracking=summary` and
`jcmd <pid> VM.native_memory summary`. To settle software versus hardware decoding, read
`hwdec-current` from `logs/debug/player-logs.jsonl` (logged once, 30 s into a stream).

## Scenarios

| ID | What |
|---|---|
| S1 | Cold start to idle Home, 5+ plugins installed, upgraded from an "unlimited" install |
| S2 | Switch provider twice |
| S3 | 1080p HLS for 10 minutes |
| S4 | 4K progressive file for 5 minutes |
| S5 | Browse, 3 pages, then Back |
| S6 | Leave the player |

## Baseline (measured 2026-11-22)

Machine: 16 cores, 31 GB RAM, Linux (Wayland, Mesa), release build of 0.5.2. Data: a **copy** of a
real install (CineStream + MegaProvider, `plugin_worker_limit = 0` = unlimited), debug logging on.
Peak of the whole process tree, PSS:

| Scenario | Peak total MiB | By role (MiB) | JVMs | Notes |
|---|---|---|---|---|
| S1 idle Home, 2 plugins | **1338.5** | app 896.5, jvm 442.0 | 2 | stable over 120 s; second run: app 893 MiB |
| S1 after step 3b-1 (10 posters per collapsed shelf, expanded pages of 20) | **816.8** | app 356.3, jvm 460.4 | 2 | app process -540 MiB (-60%) vs the row above; same data copy, same binary otherwise |
| S1 with profiles, Auto on this 31 GB PC (High-end: 4 resident plugin processes) | 801.0 | app 370.5, jvm 430.5 | 2 | same as before for this PC, as intended: a big PC is not restricted |
| S1 with profiles, Low-end forced | **639.2** | app 361.3, jvm 277.9 | 1 | one resident plugin process: -162 MiB vs Auto here |
| S1 after 3b picture store + tile decode, Low-end | **526.8** (settled 502) | app **159.8**, jvm 367.0 | 2 at peak | app process 361 -> 160 MiB. The JVM peak shows two plugin processes alive while plugins load at start; it is the limit+1 problem planned for step 1d |
| S1 after 3b picture store + tile decode, Auto (High-end) | 675.4 | app 227.7, jvm 447.7 | 2 | app process 370 -> 228 MiB |
| S1 Low-end, cold plugin conversion cache, lean JVM flags (1b) | **369.7** | app 233.2, jvm **136.4** | 2 | the same run with the old flags (`CS_JVM_PROFILE=legacy`): total 595.4, jvm 448.1. Both plugins converted and loaded with no errors; same warnings, same 43 plugin calls, CineStream's Home loaded |
| S3 player alone (`t4_memory_playback`, 1080p H.264 30 fps, 128 MB cache, embedded renderer, UI-style frame copy) | 205-240 pss | before play 49 | | cache share: 188 MiB at 32 MB, ~205 at 128 MB, 301 at 256 MB; the rest is decode and frames. Whole-app S3 estimate: S1 Low-end 364 + ~155 = ~520 MiB, under the 700 MiB budget (an estimate, not an end-to-end run) |
| S1 Low-end with the lazy player (2f) | **320.5** | app 183.6, jvm 136.8 | 1 | no player is started until a video plays (log: 0 starts); -44 MiB vs 364 |
| S2, S4-S6 | not measured yet | | | |

Findings from S1 (these replace the estimates in the first version of the plan):

* **The app process is the larger part, not the JVMs.** 893 MiB in `cloudstream` itself against about
  221 MiB per JVM (two JVMs). More installed plugins add roughly 220 MiB each when all stay resident.
* The 893 MiB is **live anonymous memory**: 850 MiB anon, 122 MiB file-backed. It sits in about 11
  blocks of 30 MiB or more (many of 64 MiB) plus a 150 MiB heap.
* It is **not** the GL driver or the allocator: forcing Slint's software renderer left it at 863 MiB;
  `MALLOC_ARENA_MAX=2` gave 875 MiB; making every block of 128 KiB or more an `mmap` that is returned
  on free (so freed memory cannot be hoarded) gave 845 MiB, and 840 MiB with one arena.
* Without the sidecar and plugins (three accidental runs where the sidecar was not found, Home empty)
  the same process used roughly 80-170 MiB. So about 750 MiB comes from what plugins' Home content
  brings in.
* Hypothesis, to be confirmed by the first Phase 3 step: decoded posters. A 360x540 RGBA poster is
  0.78 MB; the artwork cache held 656 images, and `hydrate_poster_images` asks for every poster of
  every shelf, so about 600 x 0.78 MB in the Slint models plus the loader's 144 MiB cache matches.
* Playback on this machine decodes in hardware (`vaapi-copy`, from the app's own playback
  diagnostics line), so the "probably software decode" guess for the player was wrong here.

## Measurement mistakes to avoid

* Pass environment variables to `env` as separate words. An unquoted `$VAR` holding several
  assignments is not split in zsh, and the app then starts without a sidecar (a quiet, plausible
  looking, wrong number). Check `cloudstream.log` for `using JVM sidecar` and the JVM count.

## Decision log

One entry per optimization step: what changed, the alternative considered, the measured delta,
and the switch that turns it back off. A step stays only if its delta is real.

| Step | Change | Measured delta | Rollback switch |
|---|---|---|---|
| 3b-1 | A collapsed Home shelf builds and loads only its first 10 posters (`COLLAPSED_SHELF_POSTERS`); the expanded grid shows pages of 20 (`EXPANDED_PAGE_SIZE`) over a `ShelfPager` that keeps the titles' text and asks the provider for more only when it runs out; collapsing clears the expanded model (it used to keep its pictures alive). Alternative considered: scroll-window hydration of the expanded grid; a page is simpler and bounds memory by construction. | S1 app process 896 -> 356 MiB; tree 1338 -> 817 MiB. Confirms decoded posters were the main cost. | Change the two constants in `crates/ui_slint/src/shelf_pager.rs` (by decision they are fixed for every PC, not a setting) |
| S1 | Limits follow a profile (Auto reads RAM; Low-end / Balanced / High-end by name); the viewer's choices override; Settings > General has a Performance & memory card (profile, PC facts, state, warning, Reset). One-time migration drops the plugin limit the old app wrote itself. Alternative considered: a fixed 1 resident plugin process for everyone (the earlier decision): it would restrict big PCs, so it became Auto. | Low-end vs Auto on the 31 GB test PC: 639 vs 801 MiB at idle Home | Choose the "Auto" profile, or pick the limit by hand in its card (a stored choice always wins) |
| 3b-2 | One picture per poster address in a UI-side store (`ImageStore`: pinned by the screens that show it, evictable under a 64 MiB budget, least recently used first); tiles are decoded at 240 px instead of 360 (`ArtworkSize::Tile`; hero and details keep 360); the loader's own cache shrinks from 144 to 16 MiB because the store holds the tiles. Alternative considered: cancelling queued loads when a screen lets go: not done, a stale result only enters the store unpinned and is evicted by the budget. | S1 app process: 361 -> 160 MiB (Low-end), 370 -> 228 MiB (Auto) | Change `IMAGE_STORE_BYTES` (`poster_limits.rs`), `TILE_ARTWORK_WIDTH` (`poster_artwork`) or `ARTWORK_LOADER_MEMORY_BYTES` (`app_shell/service_wiring.rs`) |
| 1b | `JvmProfile::Lean` for every plugin JVM: serial collector, `-Xms16m` with heap free ratios 10/30, C1 compiler only (one compiler thread, 48 MB code cache), metaspace capped at 128 MB, direct memory at 64 MB, no perf data, exit on OOM, `MALLOC_ARENA_MAX=2` on Linux. `-Xmx512m` and the class space cap stay. Alternative considered: AppCDS first (step 1c); flags are simpler and gave most of the win. | Two plugin JVMs: 448 -> 136 MiB (-70%); whole tree 595 -> 370 MiB on a cold conversion cache | `CS_JVM_PROFILE=legacy` (the JVM's own defaults). Not verified: StreamPlay (the heaviest plugin) under the 128 MB metaspace cap; use the switch if a plugin fails to load |
| 3f | `[profile.release]`: thin LTO, one codegen unit, symbols stripped; `panic` stays `unwind` (the cleanup queue and thread isolation rely on catching a panicking task). Alternative considered: `panic = "abort"`, ruled out for that reason. | Binary 60.7 -> 39.7 MB (-35%). Idle memory: no measurable change (S1 Low-end total 364-367 MiB against about 370 before; the app/JVM split moves between runs, the total does not). Full release rebuild takes about 2.5 min. | Remove the `[profile.release]` table |
| 1d | `WorkerLease`: a plugin with a call in flight is never parked (the lease is taken in the same step as the lookup, or when the plugin is revived); choosing and parking the oldest idle plugin is one locked step; the oldest idle plugin stops *before* a new one starts (`make_room`), at load and at revive; the last call to end parks any excess. Found while stress-testing: choosing the victim and parking it under two separate locks let a new call be cut short (3 failures in 40 runs; 0 in 150 after the fix). | With the Low-end limit of 1, the plugin process count at start stays 1 (it peaked at 2 before); a search no longer fails when another plugin is revived. S1 Low-end total 364 MiB | None needed (behavior is the intended limit semantics); the limit itself is a Settings option |
| 1g | Two unbounded reads got a cap: a line from the sidecar (reply 32 MiB, log line 64 KiB; the rest of a longer line is read and dropped so the pipe stays drained) and the joined text of an HLS subtitle playlist (16 MiB, the same as an unpacked subtitle). Safety bounds: no change in normal memory. | Not measured (guards against a hostile or runaway plugin / playlist) | Raise the named constants |
| 1e (dropped) | Provider cache for zero JVMs at start was **not** built. Reading the load path showed a plugin's load has effects the app relies on (the Mega plugin adds repositories on every start; extractors register on load), so skipping it would change extension behavior (RULES goal 3). After 1d the start-up peak already equals the limit and one JVM stays resident either way, so the memory gain would be small. Revisit only if start-up time becomes the goal. | n/a | n/a |
| 1f | A quiet Cloudflare verification helper (a browser engine, 100-300 MB) closes after a limit set by the profile (Low-end 2 min, Balanced 5 min, High-end never) or the viewer's choice (1, 2, 5, 15 min, Never) in Settings > General > Plugin processes. It counts as busy while a check is pending, while any command or status passed within the limit, and while a browser-fetch file in the exchange folder is newer than the limit. The decision runs on the watcher thread that routes requests, so a request cannot arrive in the middle of it. Trade-off: a site verified in the helper may be verified again after the helper restarts. | Not measured: it needs a Cloudflare-protected site; the decision logic is covered by tests | Choose `Never` in Settings (or the High-end profile) |
| 2b+2c | The render thread fills a recycled pixel buffer instead of allocating one per frame (`FrameShared::publish` hands back the previous picture's buffer when no reader holds it); the render width is a multiple of 16 so mpv writes tight rows straight into the final layout (no padding strip and no second copy); alpha is set in place; the render height is capped at 1080 as well as the width at 1280. Alternative considered: a `FramePool` type: the shared slot already knows when a buffer is free, so no second owner is needed. | Same harness, same video, 40 s: CPU 9.3 -> 7.3 s (-21%), pss 240 -> 237 MiB (-3). A CPU and latency win more than a memory win | Revert the commit |
| 2f | libmpv is loaded and initialized on the first play (`DeferredPlayer`: idle answers before, volume / cache / languages / picture size / verbose log remembered and applied once before the first video; a failed start is retried at the next play). `probe_libmpv` loads and unloads the library at start-up so a missing libmpv is still reported there. Race tests: one start for concurrent first plays (seen failing when made non-atomic), no setting lost while starting. | S1 Low-end idle total 364 -> 320 MiB (-44) | Build `MpvPlayer::auto_detect` directly in `app_shell::service_wiring::build_playback` |
| 2e | Zero-copy video: with `CS_VIDEO_RENDERER=gpu` mpv draws into a texture of the window's OpenGL context and Slint shows it as it is (`GlVideoTarget`, `mpv_render_api/gl.rs`, `mpv_gl_output.rs`, `ui_slint::bridge::gl_video`). No picture is copied into memory (the software path holds two 3.5 MiB buffers per frame at 1280x720; this path renders at the window's size on the GPU). **Verified by the owner on Linux/Wayland:** a 3840x2160 HEVC stream with hardware decoding (`vaapi-copy`), 30 s of playback, a seek and a window resize; the picture was upside down at first (texture origin declared bottom-left) and right after declaring it top-left; clean shutdown, mpv's render context freed with the GL context current. **Not measured:** the memory and CPU difference against the software path. **Not verified:** Windows, other drivers, hardware decoding through the GL interop (`vaapi` without copy). | Not measured yet | Leave `CS_VIDEO_RENDERER` unset (software path, the default) |
