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

## Nightly check

`.github/workflows/quality-nightly.yml` job `player-memory` installs libmpv and ffmpeg, generates a
60 s 1080p clip and runs `t4_memory_playback` with `CS_BENCH_MAX_PSS_MIB=400` (the player alone, 128 MB
cache; about 240 MiB measured on a developer PC). It guards the player part of S3 against creep. The
whole-app scenarios (S1, S3 with the plugin runtime and a window) need a display and a plugin
sidecar, so they are run by hand with `scripts/measure_memory.py` before a release.

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
| S3 whole app, 4K HEVC (hardware `vaapi-copy`), Low-end profile, **software output**, last 60 s of playback, one run | per process kind: app 712.3 MiB / 60.6 % CPU, jvm 182.2 / 0.0 %, browser_helper 1159.1 / 1.8 % | | | the verification helper (a Cloudflare check started by the movie's plugin) is excluded from the comparison; see the finding below |
| S3 same, **GPU output** (zero-copy), same video and steps, one run | app 712.2 MiB / **12.8 % CPU**, jvm 177.9 / 0.1 %, browser_helper 1121.0 / 7.2 % | | | app process CPU **-79 %** (60.6 -> 12.8 %); app memory **unchanged** (712.3 -> 712.2 MiB) |
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

## Finding: the Cloudflare verification helper can dwarf everything else

During a real playback session (a recent movie whose plugin sources sit behind Cloudflare, 4K HEVC,
software output, 2026-11-22) the whole tree measured **2000-2400 MiB at 70-100 % CPU**, and
**1494 MiB of it was `browser_helper`**: the verification helper's WebKit engine (the earlier
estimate was 100-300 MB). The player and the app together were under 730 MiB. The helper starts when a
source needs a check (`verification helper started` in the log) and the idle close (step 1f) only
applies once it is quiet, so while checks are running it is the largest consumer by far. The harness
now reports memory and CPU per kind of process (`window_roles`) so a comparison can leave it out.
Measured per process (2026-11-22, `window_processes`, software output, a movie whose sources sit behind
Cloudflare, 3 minutes): the helper tree held **1023 MiB at its peak** and stayed there until the app closed:
**7 `WebKitWebProcess` processes (976 MiB, about 140-165 MiB each, one per window, not one shared)**, one
`WebKitNetworkProcess` (47 MiB) and `cloudstream-verify` itself (92 MiB). Seven different sites asked for a
check in 30 seconds; the helper log shows up to 6 windows open at once, every one that passed (`Passed`) kept
for the rest of the session, with no release. So the cost is the number of retained windows times one browser
process each, which is what step H1 (bounded verified contexts) targets. Steps H1-H4 in the decision log answer it: a retained page was about 300-460 MiB and no browser setting changed that (H2), so a passed window is released when unused (H1) and, while kept, shrunk to a small page in a fresh process (H4). Measured on 6 real heavy pages: 1.96 GiB before, 318 MiB at the Low-end limit after. A window that has just passed, and one that is checking, still hold the real page, so a burst of many sites can briefly reach the old figure. Also note the player itself is larger on 4K than the 1080p estimate: the app process held about 712 MiB while playing 4K HEVC at a 128 MB cache.

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
| 2e | Zero-copy video: with `CS_VIDEO_RENDERER=gpu` mpv draws into a texture of the window's OpenGL context and Slint shows it as it is (`GlVideoTarget`, `mpv_render_api/gl.rs`, `mpv_gl_output.rs`, `ui_slint::bridge::gl_video`). No picture is copied into memory (the software path holds two 3.5 MiB buffers per frame at 1280x720; this path renders at the window's size on the GPU). **Verified by the owner on Linux/Wayland:** a 3840x2160 HEVC stream with hardware decoding (`vaapi-copy`), 30 s of playback, a seek and a window resize; the picture was upside down at first (texture origin declared bottom-left) and right after declaring it top-left; clean shutdown, mpv's render context freed with the GL context current. **Measured against the software path** (see the S3 comparison below). **Not verified:** Windows, other drivers, hardware decoding through the GL interop (`vaapi` without copy). | Not measured yet | Settings > General > Video output > Software (the default); `CS_VIDEO_RENDERER=gpu|cpu` overrides the setting |
| 2e (measured) | GPU versus software output on a 4K HEVC stream with hardware decoding (`vaapi-copy`), same movie and steps, 128 MB cache profile, last 60 s of one run each, the browser helper left out. | App-process CPU 60.6 -> 12.8 % (-79 %): the software path scales every 4K picture to the window size and converts it on the CPU, and the GPU path does not. App memory is the same (712.3 vs 712.2 MiB): the removed buffers are a few MiB against a player that holds the decoded 4K surfaces and the demuxer cache. So zero-copy is a CPU and battery win, not a memory win. One run each; the 4K stream and the helper make this a worst case for the software path. | Settings > General > Video output > Software |
| H1 | Verified browser windows are released when nobody needs them (`context_reaper.rs`): a passed window unused for 2 minutes, and above the limit the least recently used; never one that is being checked, is fetching, or was used in the last 30 seconds. The limit is the setting "Verified browser windows kept" (Low-end 2, Balanced 4, High-end 8; `--max-windows`, taken when the helper starts). A released window costs the sidecar one re-check (`gone`, then "verifying again"). Alternative considered: a hard cap that makes new checks wait; it can stall a plugin, so the cap is a steady-state bound and a burst may briefly exceed it. | Real WebKit, the helper driven directly with 6 real pages (a scratch script that sends `verify` commands on its stdin and samples the process tree, one run each): no release 6 web processes 1.79-1.82 GiB, tree 1.93-1.97 GiB; limit 2, 30 s after the last page: **2 web processes 0.93 GiB, tree 1.08 GiB**; idle for 2 minutes: **0 web processes, tree 195 MiB**. The retained windows of heavy news pages are about 460 MiB each, so a limit of 2 can still hold about 0.9 GiB. | Set "Verified browser windows kept" to 8 (and the helper idle to Never) |
| H4 | A passed window that has been quiet for 10 seconds is shrunk (`context_slimming.rs`): its browser is replaced by a new one holding a tiny page of our own, with the site's origin as its base address so it can still run browser fetches (Linux; dropping the old browser destroys its widget and so every process the page had. WebView2 is created asynchronously and this build cannot test replacing it, so Windows navigates to the site's `robots.txt`). Two earlier versions failed in real sessions and are why it is built this way: loading `robots.txt` left about 178 MiB in each kept window, and ending only the window's web process (then loading the tiny page) left an old heavy process next to the new small one for every window that had passed a Cloudflare challenge (288 and 323 MiB, one growing; found with the harness's per-process rows, reproduced on two public challenge pages, gone after replacing the browser: 3 processes 659 MiB -> 2 processes 149 MiB). The window stays `Passed` and keeps serving browser fetches from the small document of the same origin; cookies live in the network process and are kept. If the small page is challenged the window is dropped and the sidecar verifies again. Alternatives measured: navigating alone frees only about 11 % (WebKit keeps the heap; with the page cache off 21 %, with a memory limit too 25 %); a window that only ever loads `robots.txt` costs about 60 MiB, which is what recycling reaches. | Same 6 real pages, helper driven directly: heavy **1.96 GiB** tree; after shrinking **0.49 GiB** (web processes 1.82 GiB -> 0.35 GiB, 59 MiB each); all 6 browser fetches still answered 200 afterwards. Together with H1 at the Low-end limit of 2: **318 MiB** settled (2 web processes 161 MiB, network 83 MiB, helper 75 MiB); limit 8: 491 MiB. Not measured on Windows. | A window that is fetching or was used in the last 10 seconds is never shrunk; there is no switch: remove the `slim_if_quiet` call in `helper_app.rs` |
| H2 (dropped) | A low-memory browser profile: WebKit memory pressure limit (256 and 128 MB), `DocumentViewer` cache model, page cache off. Built, measured, removed. | 6 real pages, same run each: web processes 1.81 GiB (no tuning), 1.85 GiB (256 MB), 1.84 GiB (128 MB): **no effect**; only the network process fell, by about 15 MiB. The memory is live page heaps, which pressure handling cannot free. Media, WebGL, WebAudio and canvas switches were not tried: they change what the Cloudflare check sees and a challenge page loads no media. | - |
| H3 | A helper that serves browser fetches is not closed as idle: it prints a `fetched` status for every fetch it takes, and the watcher counts it as activity. Before, the app never saw those requests (the helper answers them and the sidecar deletes the files), so a helper in use could be closed after the idle limit and the next fetch re-verified. | Test: the helper stopped after 7 beats without the fix, kept serving with it | - |
| 1c (skipped) | AppCDS for the plugin JVMs. Not built and not measured: the lean flags (1b) took two JVMs from 448 to 136 MiB, and an archive adds a file that one JVM must create and that goes stale quietly. Reopen only if plugin start-up time becomes the problem. | - | - |
| 2a (skipped) | Decoder thread and hardware-decode profile. Hardware decoding (`vaapi-copy`) was already in use on the test machine, so the premise was refuted there. | - | - |
| 3b cancel-on-release (skipped) | Cancelling queued poster loads when a screen lets go. A late result enters the store unpinned and is evicted by the 64 MiB budget, so memory stays bounded; cancelling would only save network and decode work, for more concurrent code and race tests. | - | - |
| 3d (partly done) | Posters decode at 240 px (3b-2). Not done: a decode concurrency limit, RGB8 storage and disk thumbnails. Memory is already bounded by the store, so these would add code for little. | - | - |
| 3e (skipped) | Shared strings and one plugin catalog. A title's text is about 1 KB, so 500 titles are about 0.5 MB; the change would touch `core_model` and every crate that builds its types, and must keep decoding the recorded sidecar replies. Reopen only if a heap profile shows text data above about 10 MiB. | - | - |
| 3g (skipped) | A different allocator (`mimalloc`). The baseline tested allocator fragmentation and ruled it out; the large sources were then removed directly. A new dependency and different behavior on Windows for a guess. | - | - |

## Status of the memory plan (2026-11-22)

Complete for the agreed scope. The skipped steps above were skipped with a reason each. Search, Browse and
Library poster counts stay uncapped by decision. Measured only S1 and S3 (S2 and S4-S6 were not run).
Not verified: Windows for every step, the GPU path outside Linux/Wayland, the `player-memory` nightly job
on GitHub. Open work outside the plan: the Cloudflare verification helper (see the finding above).
