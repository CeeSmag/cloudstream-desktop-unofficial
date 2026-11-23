# Quality Assurance & Release Verification Runbook (`docs/QA.md`)

Repeatable QA procedures, regression matrix, failure-injection checks, and release sign-off criteria for `cloudstream-desktop-unofficial` (`app_shell` + `ui_slint`).

See also:
- [RULES.md](../RULES.md) — coding standards, §8 gates, §9 test tiers & mandatory race tests
- [docs/RELEASING.md](RELEASING.md) — build, minisign verification, and artifact packaging
- [docs/LOW_END_RETEST.md](LOW_END_RETEST.md) — constrained-hardware (<= 8 GB RAM) retest checklist

---

## 1. Tiered Verification Matrix

| Tier / Gate | Command | Scope | Frequency |
|---|---|---|---|
| **Format** | `cargo fmt --all -- --check` | Workspace formatting | Every commit / PR |
| **Clippy (Linux / Windows)** | `cargo clippy --workspace --all-targets -- -D warnings` (`-A clippy::cognitive_complexity` with Wayland on Linux; strict with `--no-default-features`) | Lints, zero panics/unwraps, complexity | Every commit / PR |
| **Unused Deps & Docs** | `scripts/check_dead_code.sh` | `cargo-machete` + `-D rustdoc::broken_intra_doc_links` | Every commit / PR |
| **Architecture & Unsafe** | `python3 scripts/check_architecture.py` & `python3 scripts/check_unsafe_audit.py` | One-way crate graph, 0 cycles, `RULES.md` §8 unsafe allowlist + `// SAFETY:` | Every commit / PR |
| **File Lengths & Scripts** | `python3 scripts/check_file_lengths.py` & `shellcheck scripts/*.sh` | 600-line production cap, script unit tests, shell hygiene | Every commit / PR |
| **Supply Chain** | `cargo deny check` & `cargo audit` | License allowlist (`deny.toml`), advisories, yanked crates | Every commit / PR |
| **T1 / T2 / Race / Prop / Slint / BDD** | `cargo test --workspace` (or `cargo nextest run --workspace`) | Unit, integration, deterministic race, `proptest`, headless Slint render/constraints/interaction, Cucumber `.feature` journeys | Every commit / PR |
| **Coverage** | `scripts/coverage.sh` | `cargo-llvm-cov` line/branch/function report (`target/coverage/`) + per-crate thresholds | Nightly / Pre-release |
| **Mutation** | `scripts/run_mutants.sh --diff` / `--core` | `cargo-mutants` on PR diff or 6 core crates (`core_model`, `skip_times`, `title_cache`, `subtitle_store`, `stream_proxy`, `playback_session`) | PR diff / Nightly |
| **Unsafe Transitive Audit & Metrics** | `cargo geiger` & `python3 scripts/quality_metrics.py` | Dependency unsafe footprint + per-crate LOC/tests/coverage/mutants JSON & Markdown | Nightly / Pre-release |
| **Fuzz Corpus & Benchmarks** | `scripts/run_fuzz_sanity.sh` & `just bench-quick` | Parser stress/corpus replay + Criterion hot-path regression check | Nightly / Pre-release |
| **Player memory** | `t4_memory_playback` (nightly job `player-memory`) | Peak proportional memory of the real libmpv playing a generated 1080p clip, against a budget | Nightly |
| **T3 / T4 Live** | `just live`, `python scripts/check_verify_window.py <helper>`, `cargo test -p cloudflare_bridge --test t4_windows_saved_passes -- --ignored` (Windows) | Real sidecar + libmpv playback, native WebView2/WebKitGTK cookie/window checks, and Windows WebView2 saved-pass lifecycle | Pre-release |

---

## 2. Pre-Release Smoke Checklist

Run on **both** Linux (`CloudStream-<ver>-x86_64.AppImage`) and Windows (`CloudStream_<ver>_x64-setup.exe` + `CloudStream_<ver>_x64_portable.exe`).

### Startup & Packaging
- [ ] Verify release signature before smoke testing (`minisign -Vm SHA256SUMS -p crates/app_updater/update_signing_key.pub && sha256sum -c SHA256SUMS`).
- [ ] Clean-data cold launch completes splash screen without crash or blank frame; window maximizes and restores cleanly without shrinking (`window_constraints_tests` covers layout constraints).
- [ ] Windows portable exe unpacks into `%LOCALAPPDATA%\CloudStreamRs\runtime-<version>-<payload_id>`, launches with the CloudStream icon, and exits cleanly with zero orphan `java` / `cloudstream-verify` processes.
- [ ] Windows without Vulkan SDK / GPU driver loads `libmpv-2.dll` using bundled `vulkan-1.dll` (see `docs/RELEASING.md`).

### Core User Journeys
- [ ] **Extensions**: Add repository URL → browse plugin list → install `.cs3` → toggle Trust/Untrust → confirm providers appear in top bar → uninstall and verify providers disappear without restart.
- [ ] **Catalog & Library**: Switch provider → Home shelves load with hydrated posters → Search returns results → open Details → toggle Bookmark → verify item appears in Library.
- [ ] **Playback & Resume**: Start movie/episode → playback opens best working stream → seek, pause/resume, adjust volume, switch audio & subtitle tracks → leave player → verify item appears in Continue Watching at saved position → reopen and resume.
- [ ] **Skip & Next Episode**: Verify Skip Intro / Skip Credits / Next Episode prompt appears at expected timestamps and advances episode cleanly.
- [ ] **Settings & Diagnostics**: Toggle **Write detailed debug logs** on → trigger Home + Player action → inspect `<data>/logs/debug/*.jsonl` for valid JSON Lines across categories → run **Clean up storage now** and confirm UI stays responsive.
- [ ] **Low-End Hardware**: Complete `docs/LOW_END_RETEST.md` (Low-end performance profile: 1 resident plugin process, 128 MB video cache) and compare process-tree memory with the budgets in `docs/MEMORY.md`.

---

## 3. Failure-Injection Playbooks

| Scenario | How to Inject | Expected App Behavior | Automated Coverage |
|---|---|---|---|
| **Sidecar JVM crash / hang** | Kill a running plugin `java` process mid-session (or trigger timeout) | App stays alive; next provider call restarts the worker and replays `startup_calls`; user sees a human-readable error if retries exhaust. | `extension_runtime/tests/sidecar_process_transport.rs`, `t2_streaming_search_protocol.rs` |
| **Broken stream fallback** | Return 404/500/invalid body on highest-quality stream URL while lower-quality mirror returns 200 | `playback_session` probes candidates, marks broken link, starts playback on the highest-ranked working link, persists health to `TitleCache`. | `playback_session` tests, `tests/bdd/features/stream_playback_and_fallback.feature` |
| **Cloudflare verification** | Hit a CF-challenged host; test **Decline**, **Allow**, saved-pass reuse across restarts/subdomains, and refusal (`rejected=<id>`) | Decline unblocks sidecar promptly; Allow launches `cloudstream-verify`, saves domain/host-only passes outside `<data>/sidecar`, reuses on restart without launching helper, and retires refused passes without marking the origin browser-only. | `cloudflare_bridge/tests/t2_watch.rs`, `t2_saved_passes.rs`, `t4_windows_saved_passes.rs`, `scripts/check_verify_window.py`, `ui_slint/tests/ui_interaction_tests.rs` |
| **Corrupted cache recovery** | Truncate/overwrite a JSON file in `<data>/sources/`, corrupt a subtitle in `<data>/subtitles/`, or place invalid image bytes in `<data>/artwork/` | App logs `warn!`, treats entry as cache miss, re-fetches cleanly without panic or UI freeze. | `title_cache`, `subtitle_store`, `poster_artwork` unit & integration tests |
| **Corrupted / unreadable HTML subtitle** | Serve an HTML Cloudflare/404 page with HTTP 200 at a subtitle URL | `normalize_subtitle` rejects HTML payload; broken track is never attached to mpv. | `subtitle_store/src/normalize_subtitle_tests.rs`, `tests/bdd/features/subtitles_pipeline.feature`, `fuzz/fuzz_targets/subtitle_normalize.rs` |
| **Storage cap pressure / wipe** | Fill `<data>/artwork/` past cap or trigger full data wipe in Settings | Scheduled/manual cleanup trims regenerable caches without touching `cloudstream.db` bookmarks/progress; wipe confirmation removes data cleanly. | `app_services/tests/storage_cleanup_tests.rs`, `cleanup_scheduler_race_tests.rs` |

---

## 4. Release Sign-Off Criteria

A release tag (`v*`) may be published only when:
1. All **fast per-commit gates** (`just gates`) pass on both **Windows** and **Linux** CI, plus the Windows **Sidecar Kotlin/Gradle** job passes.
2. Zero `cargo deny check` or `cargo audit` failures.
3. Whole-workspace coverage (`scripts/coverage.sh`) meets or exceeds every per-crate threshold in `scripts/coverage_config.json`.
4. PR diff mutation testing (`scripts/run_mutants.sh --diff`) has zero unexplained surviving logic mutants (any equivalent mutants documented below).
5. All **Pre-Release Smoke Checklist** items pass on Linux AppImage and Windows Installer + Portable builds.
6. Draft GitHub Release contains all expected binaries plus valid `SHA256SUMS` and `SHA256SUMS.minisig`.

---

## 5. Documented Coverage Exclusions & Equivalent Mutants

- **Coverage / Mutation Exclusions**: Explicitly listed in `scripts/coverage_config.json` and `.cargo/mutants.toml`. Restricted solely to thin `libmpv` FFI bindings, Win32 AppContainer / Linux Landlock syscall wrappers, native WebView window loops, Slint generated bindings / GUI bridge glue, and `main()` wiring entrypoints.
- **Equivalent Mutants**: Recorded here after mutation runs when a surviving mutant cannot change observable behavior:
  - `title_cache/src/title_prune.rs` (`log_report` & `kept_title.rs:41` / `title_cache.rs:110` `if removed > 0 { tracing::info!(...) }` guards): log-only side effects that do not alter return values or disk state.
  - `subtitle_store/src/subtitle_prefetch.rs` (`Shared::log_summary`): log-only summary emitter.
  - `skip_times/src/skip_fetcher.rs` (`UreqSkipFetcher::get_text`, `MAX_BYTES` arithmetic): live-network HTTP client covered by `tests/live_lookup.rs` (`#[ignore]`).
  - `cloudflare_bridge/src/disk_pass_book.rs` (`open_read_lock` match guard `is_shared_fallback_error(error.kind())` on Windows): Win32 `CreateFileW` maps both directories and read-only files to `PermissionDenied`, whereas Unix distinguishes `EISDIR` vs `EACCES`; the pure predicate itself is 100% killed by unit tests.
  - `cloudflare_window/src/outbox.rs` (`Outbox::status`) & `webview2_cookies.rs` (`read_cookies`): thin 1-line stdout / WebView2 COM hand-overs exercised end-to-end by `scripts/check_verify_window.py` and `t4_windows_saved_passes`.
