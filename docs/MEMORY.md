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
| 3b-1 | A collapsed Home shelf builds and loads only its first 10 posters (`COLLAPSED_SHELF_POSTERS`); the expanded grid shows pages of 20 (`EXPANDED_PAGE_SIZE`) over a `ShelfPager` that keeps the titles' text and asks the provider for more only when it runs out; collapsing clears the expanded model (it used to keep its pictures alive). Alternative considered: scroll-window hydration of the expanded grid; a page is simpler and bounds memory by construction. | S1 app process 896 -> 356 MiB; tree 1338 -> 817 MiB. Confirms decoded posters were the main cost. | Raise the two constants in `crates/ui_slint/src/shelf_pager.rs` (no runtime switch yet; they move to `MemoryBudget` in step 3a) |
