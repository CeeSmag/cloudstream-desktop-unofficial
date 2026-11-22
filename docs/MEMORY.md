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
| S2-S6 | not measured yet | | | |

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
