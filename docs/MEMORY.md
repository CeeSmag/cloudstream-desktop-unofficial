# Memory Budget & Optimization Summary

Target: run the full process tree (`cloudstream` + plugin JVMs + `cloudstream-verify` browser processes) comfortably on a **4 GB PC**.

## Acceptance Budgets (Whole Process-Tree PSS)

| Scenario | Description | Budget |
|---|---|---|
| **S1** | Cold start to idle Home (1 resident plugin, Low-end profile) | **<= 450 MiB** (measured: **320.5 MiB**) |
| **S3** | Playing 1080p HLS for 10 min (128 MB cache) | **<= 700 MiB** (est. ~520 MiB; player alone: 205–240 MiB) |

*(Other scenarios S2 switch provider, S4 4K progressive, S5 Browse 3 pages, S6 leave player are tracked without fixed budgets.)*

## How to Measure

```bash
# Always measure with a scratch data dir (never the real user profile)
CS_DATA_DIR=$(mktemp -d) python3 scripts/measure_memory.py --launch --label S1 --duration 120 -- ./cloudstream
python3 scripts/measure_memory.py --pid <pid> --label S3 --duration 600 --max-total-mib 700
just memory --pid <pid> --label S1
```

- **Linux** reports PSS (`/proc/<pid>/smaps_rollup`, shared pages counted once); **Windows** reports working set (upper bound) — do not compare across OSes.
- Pass env vars to `env` as separate words; verify `using JVM sidecar` in `cloudstream.log`.
- Nightly CI (`.github/workflows/quality-nightly.yml` `player-memory`) runs `t4_memory_playback` with `CS_BENCH_MAX_PSS_MIB=400` on a generated 1080p clip.
- Process names under `cloudstream` (Linux AppImage): `cloudstream` (app + libmpv), `cloudstream-jvm` (symlink to `java`; Windows shows `java.exe`), `cloudstream-ver` (helper), `WebKitWebProces` / `WebKitNetworkPr` (WebKit children).

## Measured Progress (Linux Wayland, 16-core / 31 GB, v0.5.2, 2026-11-22)

| Milestone | Peak Tree PSS | Breakdown |
|---|---|---|
| Initial S1 baseline (2 plugins, unlimited) | **1338.5 MiB** | app 896.5, jvm 442.0 |
| After shelf cap (3b-1: 10/row, 20/page) | **816.8 MiB** | app 356.3 (-60%), jvm 460.4 |
| After Low-end profile (S1: 1 resident plugin) | **639.2 MiB** | app 361.3, jvm 277.9 |
| After `ImageStore` + 240 px tiles (3b-2) | **526.8 MiB** (502 settled) | app 159.8, jvm 367.0 |
| After lean JVM flags (1b, cold cache) | **369.7 MiB** | app 233.2, jvm 136.4 (-70%) |
| After lazy player init (2f, `DeferredPlayer`) | **320.5 MiB** | app 183.6, jvm 136.8 (1 JVM) |
| S3 4K HEVC `vaapi-copy`: Software vs GPU zero-copy (2e) | App **712.3 vs 712.2 MiB** | App CPU **60.6% -> 12.8%** (-79%) |
| Cloudflare helper (6 real heavy pages / real session, H1–H4) | **1960 -> 318 MiB** / **1023 -> 196 MiB** settled | Burst peak unchanged (~1.1 GiB for first 25–30 s) |

**Key findings:** Decoded posters (not allocator/GL) dominated idle app RAM; hardware decoding (`vaapi-copy`) was already active; GPU zero-copy is a major CPU/battery win (-79% CPU on 4K) rather than a RAM win; retained Cloudflare browser windows (`~150–460 MiB` each) dwarfed the rest of the app until bounded and slimmed.

## Decision Log

| Step | Change | Measured Delta | Rollback / Setting |
|---|---|---|---|
| **S1** | Auto RAM-tier presets (`memory_budget`: Low-end <=6 GB, Balanced <=12 GB, High-end >12 GB) + user overrides in Settings > General. | Low-end vs Auto: 639 vs 801 MiB | Choose **Auto** or custom values in Settings |
| **1b** | `JvmProfile::Lean` (SerialGC, `-Xms16m`, C1-only, 48 MB code cache, 128 MB metaspace, 64 MB direct mem, `MALLOC_ARENA_MAX=2` Linux). | 2 JVMs: 448 -> 136 MiB (-70%) | `CS_JVM_PROFILE=legacy` |
| **1d** | `WorkerLease`: never park in-flight plugins; stop oldest idle plugin *before* starting a new one (`make_room`). | Peak JVMs stays at limit (1 on Low-end) | Adjust **Plugin processes** in Settings |
| **1f** | Close quiet Cloudflare helper after idle TTL (Low-end 2 min, Balanced 5 min, High-end never). | Covered by unit/T2 tests | Set **Never** in Settings |
| **1g** | Bounded pipe/playlist reads (sidecar reply 32 MiB, log line 64 KiB, HLS subtitle 16 MiB). | Safety cap | Named constants |
| **2b+2c** | Recycled tight-stride (16-px aligned) render buffers (`FrameShared::publish`), 1280x1080 cap. | CPU -21% (9.3 -> 7.3 s / 40 s), PSS -3 MiB | Revert commit |
| **2e** | Zero-copy OpenGL video (`GlVideoTarget`, `mpv_gl_output.rs`, `gl_video.rs`). Verified Linux/Wayland; unverified Windows. | 4K app CPU 60.6% -> 12.8% (-79%); RAM flat | Settings > Video output > Software / `CS_VIDEO_RENDERER=cpu` |
| **2f** | `DeferredPlayer`: load/init libmpv on first play (`probe_libmpv` still checks presence at startup). | Idle Home 364 -> 320 MiB (-44 MiB) | Wire `MpvPlayer::auto_detect` directly |
| **3b-1** | Collapsed Home rows load 10 posters; expanded `ShelfPager` pages 20; collapse frees models. | App 896 -> 356 MiB (-60%) | Constants in `ui_slint/src/shelf_pager.rs` |
| **3b-2** | UI `ImageStore` (64 MiB LRU, pinned per screen), 240 px tile decode, loader cache 144 -> 16 MiB. | App 361 -> 160 MiB (Low-end) | `poster_limits.rs`, `poster_artwork`, `service_wiring.rs` |
| **3f** | Release profile: thin LTO, `codegen-units = 1`, `strip = true`, `panic = "unwind"`. | Binary 60.7 -> 39.7 MB (-35%) | Remove `[profile.release]` |
| **H1** | `context_reaper.rs`: release passed helper windows after 2 min idle or LRU above `--max-windows` (2/4/8; 30 s grace). | 6 pages: 1.97 -> 1.08 GiB (0 web procs after 2 min: 195 MiB) | Set **Verified browser windows kept** to 8 |
| **H3** | Helper prints `fetched` status on browser fetches so active helpers are not closed as idle. | Verified by T2 test | — |
| **H4** | `context_slimming.rs`: after 10 s quiet, Linux replaces browser with tiny same-origin page (destroying heavy process); Windows navigates to `/robots.txt`. | 6 pages: 1.96 GiB -> 318 MiB settled (real session: 1023 -> 196 MiB) | Remove `slim_if_quiet` in `helper_app.rs` |

**Dropped / Skipped:** `1e` (zero-JVM startup cache: breaks Mega/extractor load side effects), `H2` (WebKit memory pressure limits: no effect on live page heaps), `1c` (AppCDS: 1b already cut 70%), `2a` (already HW-decoding), `3b` cancel-on-release / `3d` RGB8 thumbnails (store already bounds RAM), `3e` string interning (<0.5 MB), `3g` `mimalloc` (allocator ruled out).

## Remaining Gaps
- S2, S4–S6 whole-tree measurements not yet run; Windows memory figures, Windows GPU zero-copy (`2e`), and Windows `robots.txt` slimming (`H4`) not yet benchmarked.
