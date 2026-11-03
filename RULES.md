# RULES.md — Rust Coding Standards for AI Contributors

Binding rules for any AI agent writing code in this project (the Rust rewrite of CloudStream).
Read this file fully before every task. If a rule conflicts with a request, say so and ask; never silently break a rule.

---

## 1. Project Goals (non-negotiable)

1. **UI** resembles the original CloudStream UI (reference: `app/` and `shared/` in the Kotlin source), implemented in Rust.
2. **Video player is mpv** (libmpv). No other player backend.
3. **Extensions run at 100% fidelity**: providers, extractors, plugins, scripts, and their behavior must match the Kotlin reference.
4. **Parity is proven by tests** (see §9), not assumed. A feature is not "done" until its parity test passes.

> Open decision: the extension runtime (Wasm-recompiled vs. JVM sidecar) is chosen in the feasibility spike. Until then, code against the `ExtensionRuntime` trait (§3), never a concrete runtime.
> Verified facts for the spike: Wasmtime enabled the Wasm GC proposal by default in v47.0.0 (2026-07-20); later notes say Winch does not enable GC types by default, so use the Cranelift strategy. Kotlin/Wasm needs Wasm GC. Re-check the current Kotlin/Wasm maturity before committing to it.
> `player_mpv`: use the maintained `libmpv2` / `libmpv2-sys` bindings (the older `mpv-rs` is deprecated). The libmpv C API is generally thread-safe. mpv is GPLv2+ (the client API is ISC): confirm the licensing consequences of linking before distribution.

---

## 2. Architecture

- Cargo **workspace**, one crate per bounded concern. Suggested layout:

```
crates/
  core_model/        # pure data types (Movie, Episode, SearchResponse, ...). No I/O.
  extension_api/     # traits + host-function contracts exposed to extensions
  extension_runtime/ # loads/executes extensions behind ExtensionRuntime trait
  extension_repo/    # repository manifests, install/update/remove of plugins
  network_client/    # HTTP, cookies, headers, Cloudflare/webview-resolver abstraction
  html_parsing/      # selector/DOM layer (Jsoup-compatible behavior)
  extractors/        # one file per extractor
  player_mpv/        # libmpv wrapper only
  storage/           # settings, library, history, downloads DB
  ui_app/            # screens, widgets, theme, navigation
  app_shell/         # main(): wiring only, no logic
tests/
  t1_unit/ t2_integration/ t3_parity/ t4_end_to_end/   (or per-crate equivalents, see §9)
```

- **Dependency direction is one-way**: `ui_app → services → traits ← implementations`. `core_model` depends on nothing internal. No cycles. Enforce with `cargo deny`/`cargo-modules` in CI.
- `app_shell` is the only place where concrete implementations are wired to traits (dependency injection by constructor).
- Navigating the repo must be obvious from folder names alone; each crate has a `README.md` (purpose, public API, how to test) of ≤ 40 lines.

---

## 3. No Coupling

- Cross-crate and cross-module interaction goes through **traits or plain data types**, never through another module's internals.
- No global mutable state, no `static mut`, no hidden singletons. Pass dependencies explicitly.
- A module must be removable by deleting its folder, its `mod` line, and its wiring in `app_shell`. If that breaks unrelated code, the design is wrong.
- Public items are the minimum necessary (`pub(crate)` by default). No `pub use` re-export chains that hide where things live.
- UI never calls network, mpv, or storage directly; it calls service traits.
- Extractors and providers know nothing about the UI or player.

## 4. No God Files / God Functions

- Function: ≤ **40 lines**, one responsibility, ≤ 4 parameters (otherwise take a config/struct).
- File: ≤ **300 lines**. Split by responsibility before exceeding.
- Module `mod.rs` contains declarations and re-exports only, no logic.
- No `utils.rs`, `helpers.rs`, `common.rs`, `misc.rs`, `manager.rs` dumping grounds. Name by what it does.
- One extractor / one provider / one screen / one service per file.
- Complexity: `clippy::cognitive_complexity` is a **nursery** lint (off by default) and `too_many_lines` / `too_many_arguments` default to looser limits, so they MUST be enabled and tightened explicitly in `clippy.toml` (see §8). If splitting a function would exceed the argument limit, group parameters into a struct rather than adding `#[allow]`.

## 5. Naming (self-explanatory)

- Files: `snake_case`, describe content: `parse_m3u8_playlist.rs`, `resolve_stream_urls.rs`, `episode_list_screen.rs`.
- Functions: verb + object: `fetch_episode_list`, `parse_search_results`, `build_request_headers`. Booleans: `is_/has_/can_`.
- Types: nouns, no abbreviations (`ExtensionRepositoryManifest`, not `ExtRepoMf`). No `Data`, `Info`, `Handler`, `Manager`, `Helper` suffixes unless truly the domain term.
- Error types: `<Domain>Error` (`NetworkRequestError`, `ExtensionLoadError`).
- Tests: `<unit>_<scenario>_<expected>` e.g. `parse_m3u8_missing_header_returns_error`.
- No single-letter names except trivial iterator/closure vars. No `tmp`, `data2`, `foo`.

## 6. Logging, Errors, Fallbacks (mandatory everywhere)

- Use `tracing` for all logs. No `println!`/`eprintln!`/`dbg!` in committed code.
  - `error!` failures that affect the user; `warn!` recovered/fallback paths; `info!` lifecycle events; `debug!`/`trace!` data flow.
  - Every log line carries context fields (`provider`, `url`, `plugin_id`, `request_id`...). Never log secrets, tokens, or full cookies.
- Errors: `thiserror` for library crates, `anyhow` only in `app_shell` and tests. Every error type carries **what failed, where, and why**, and keeps its `source()`.
- **No `unwrap()`, `expect()`, `panic!`, `unreachable!` on fallible paths** in non-test code. These are *restriction* lints, off by default: deny `clippy::unwrap_used`, `expect_used`, `panic`, `unreachable`, `todo`, `indexing_slicing` in the workspace `[lints]` table; tests are exempted via `allow-unwrap-in-tests` / `allow-expect-in-tests` in `clippy.toml`. Indexing is via `.get()`.
- Every fallible operation has an explicit **handling decision**: propagate with context, retry with backoff, fall back, or surface to the user. "Ignore" must be a commented, logged decision.
- Every external boundary (network, filesystem, mpv, extension execution, parsing) has:
  1. a timeout,
  2. a logged failure path,
  3. a defined fallback (e.g. next mirror/extractor, cached result, empty-with-notice) or a clear user-visible error.
- An extension crashing/panicking/timing out must **never** crash the app: isolate, log, mark the extension as failed, continue.
- User-facing errors are human-readable; the technical detail goes to the log.

## 7. Debuggability & Changeability

- Small pure functions; side effects at the edges. Prefer returning values over mutating shared state.
- Make illegal states unrepresentable (enums, newtypes) instead of runtime flags and magic strings/numbers.
- No magic constants: named `const`s with doc comments.
- Config over code: URLs, timeouts, retry counts live in typed config structs with defaults.
- Add: new feature = new file + one registration line. Remove: delete file + that line. Modify: change one place.
- Comments explain **why**, never restate what. Public items have `///` docs with an example when non-obvious.
- Keep behavior deterministic where possible (inject clock/RNG/HTTP) so bugs are reproducible in tests.

## 8. Code Quality Gates

Must pass before every commit:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check        # licenses/advisories
```

Required `clippy.toml` at the workspace root (limits mirror §4):

```toml
too-many-lines-threshold = 40
too-many-arguments-threshold = 4
cognitive-complexity-threshold = 10
allow-unwrap-in-tests = true
allow-expect-in-tests = true
```

and in the root `Cargo.toml` `[workspace.lints.clippy]`: `unwrap_used`, `expect_used`, `panic`, `todo`, `unreachable`, `indexing_slicing`, `cognitive_complexity` = `"deny"`/`"warn"` as above, inherited by each crate with `[lints] workspace = true`.

- `#![forbid(unsafe_code)]` in every crate except `player_mpv` (FFI) and the extension runtime, where each `unsafe` block has a `// SAFETY:` comment.
- Pin dependencies; justify each new dependency in the commit message. Prefer std/small crates.

## 9. Test Tiers (T1–T4)

Every change ships with tests at the appropriate tiers. Tiers run in order; a lower tier failing blocks higher ones.

| Tier | Name | Scope | Rules |
|---|---|---|---|
| **T1** | Unit | One function/type in isolation | No I/O, no network, no sleeps. Fast (<1s/crate). Cover happy path, edge cases, each error variant. |
| **T2** | Integration | Several modules/crates together, via trait seams | Mock HTTP server (recorded fixtures), temp dirs, fake mpv. No real internet. Includes race tests (see below). |
| **T3** | Parity / Contract | New Rust behavior vs. the **Kotlin reference** | See §10. Golden files and extension-behavior comparisons. |
| **T4** | End-to-end | Real app flow: launch → search → open → resolve → play (mpv headless) | Runs against recorded or local test servers; UI driven by test harness; slow, run in CI nightly + before release. |

- Bug fix = first write a failing test at the lowest tier that reproduces it, then fix.
- Tests are as clean as production code (naming, size, no coupling).
- No flaky tests: fix or delete, never retry-until-green.

### Race tests (mandatory for concurrent code)

Any file that shares state between threads (`Mutex`, `RwLock`, `Condvar`, atomics, channels, spawned threads, async tasks) or coordinates work that can be cancelled or replaced while it runs (a video opened, closed or switched while its search, checks or subtitles are still going) **must have real race tests**, in the same change that adds or edits it. Tests that only wake a waiter after a short sleep are not race tests.

1. **Stress test**: several threads, thousands of iterations, asserting an invariant that must hold for *every* interleaving: each item delivered exactly once, nothing handed to the wrong owner, lengths of parallel lists always equal, at most one video open, the result independent of the order answers arrive in, all threads finish (join with a timeout so a deadlock fails the test instead of hanging the run).
2. **Deterministic interleaving test** for every window you can name: write the exact order of steps that used to go wrong ("old file's `FileLoaded` arrives after the switch", "video A opens its list after video B replaced it") and assert the outcome.
3. **Show the test can fail**: for a race fix, run the new test against the old behaviour (revert or mutate the fix) and confirm it fails, then restore the fix. A race test that was never seen failing proves nothing.
4. **Make the race testable**: put the check and the change under one lock, tag work with a generation so stale work is recognised, and move the logic into a function that does not need libmpv, the network or a window. Do not test by sleeping for "long enough".
5. Race tests are T2. Keep each under about two seconds, with no real network and no real player; invariants are checked after joining the threads. A test that fails once in a hundred runs is a real bug in the code or in the test: fix it, do not rerun it.
6. The same applies to the JVM sidecar: concurrent Kotlin code needs a test that interleaves threads. If the module has no test setup yet, adding one is part of the change.
7. In review of any concurrent change ask: what is checked and acted on in two separate steps? what is read before and used after another thread can change it? which order do the locks nest in? what happens if the viewer closes at this exact point?

## 10. Parity Testing (100% same behavior)

- A dedicated `tests/t3_parity/` suite proves extension and behavior equivalence with the Kotlin implementation.
- **Golden fixtures**: for each provider/extractor, record from the Kotlin reference: input (URL/query/HTML fixture) → output (`SearchResponse`, `LoadResponse`, `ExtractorLink`, subtitles, headers, referer, quality, ordering). The Rust run must match byte-for-byte on normalized JSON.
- **Behavior checks** include: request headers/cookies sent, redirect handling, error and fallback behavior, ordering of results, null/empty handling, pagination, rate-limit/retry behavior, JS interpreter outputs (`JsInterpreter` test vectors from the Kotlin tests are ported 1:1).
- The Kotlin test suites in `library/src/commonTest` are ported to Rust (T1) and their vectors reused as T3 fixtures.
- Each real extension gets a parity case; the CI report lists extensions as `PASS` / `FAIL` / `UNTESTED`. Releases require zero `FAIL`.
- Any intentional deviation is documented in `docs/parity_deviations.md` with a reason, and requires user approval.
- Never mark parity "done" from reading code; only from a passing T3 run.

## 11. Git Workflow

- Repository is initialised with `git init`; work happens on feature branches, never directly on `main`.
- **Commit on every complete set of implementation**: a coherent unit (feature/fix/refactor) with its tests passing and all §8 gates green. Not mid-way, not with failing tests, not one giant commit at the end.
- One logical change per commit. Don't mix refactor + feature + formatting.
- Message format (Conventional Commits): `type(scope): imperative summary` — types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`. Body explains *why* and lists tests added. Include the attribution trailer required by the tooling.
- Never `--no-verify`, never force-push shared branches, never commit secrets, build output (`target/`), or large binaries.
- Do not commit unless the §8 gates pass; report honestly if something was skipped or failing.

## 12. AI Working Procedure

1. Read this file and the relevant crate `README.md`.
2. State the plan briefly (files to add/change, tiers of tests).
3. Write the failing test(s) first (T1/T2, and T3 for behavior parity). Concurrent code also gets race tests (§9).
4. Implement within the size/coupling limits; add logging and error handling as you go.
5. Run the gates in §8 and the affected tiers.
6. Commit (§11). Update the crate `README.md` if public behavior changed.
7. Report: what changed, what was tested, what was **not** verified. Never claim success without having run the tests.

When uncertain about requirements (especially extension fidelity trade-offs), **ask** rather than guess.
