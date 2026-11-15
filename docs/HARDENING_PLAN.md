# Hardening plan: updater, CI gates, rule breaks, docs

Written 2026-11-15 against `459f4dd` (v0.4.2). Every finding below was checked in the code; line numbers are from that commit.

## What was found

| # | Finding | Checked | Severity |
|---|---|---|---|
| 1 | The updater installs whatever it downloads: no checksum, no signature (`update_commands.rs` `apply_update`, `download_and_finalize`, `spawn_updater_and_exit`). | Yes | **High** |
| 1a | *Extra:* `apply_update(download_url)` takes the URL **from the web page** and downloads it. Anything that can call the command (a script in the web view) can make the app download and run any file. | Yes (`update_commands.rs:411`, `api.ts:244`) | **High** |
| 1b | *Extra:* the download and the helper script go to the shared temp folder under fixed or guessable names (`CloudStream_update.<ext>`, `cs_update_<pid>.sh`). On a Linux PC with several users, `/tmp` lets another user plant or swap the file between download and `mv`. | Yes (`:249`, `:283`, `:381`) | Medium |
| 2 | `update_commands.rs` is 478 lines (limit 300), returns `String` errors, uses one-letter names (`e`, `a`, `l`, `c`, `p`, `d`). `is_version_newer` falls back to "different = newer" when a tag does not parse, which can offer a downgrade. | Yes (`:96-105`) | Medium |
| 3 | `ci.yml` runs fmt, clippy, tests, but not `cargo deny` / `cargo audit` (the `gates` target does). CI runs on Windows only, so the `#[cfg(not(windows))]` updater path and the Linux asset choice are never compiled or tested. Nothing checks the 300-line file limit: clippy's `too_many_lines` is per function. | Yes | Medium |
| 4 | Six non-test files over 300 lines: `update_commands.rs` 478, `browse_commands.rs` 369, `cloudflare_fetch.rs` 366, `settings_service.rs` 340, `cleanup_queue.rs` 333, `playback_service.rs` 305. `playback_service.rs:10` `#[allow(unused_imports)] pub use ...` duplicates the re-export `lib.rs:39` already makes. `app_state.rs:43` `#[allow(dead_code)] fn write` has no caller. (`main.rs:105` `#[allow(clippy::too_many_lines)]` on the command list is commented and justified: keep.) | Yes | Low |
| 5 | `network_client`, `html_parsing`, `extractors`: 2-line `lib.rs`, a README, still workspace members. | Yes | Low |
| 6 | `AGENTS.md` (732 lines) says "every symbol ... has been verified". It has already drifted: it says 57 IPC commands, `main.rs` registers 64. | Yes | Low |

## Order

1. **Updater** (tasks U1-U6): the only real security issue. Refactor first (U1-U2) so the signing work lands in small files, not in a 478-line one.
2. **CI** (C1-C4): so the new Unix updater code and the file limit are checked from then on.
3. **Rule breaks** (R1-R3): mechanical, can run in parallel with 2.
4. **Workspace and docs** (D1-D3).

Phases 2-4 do not touch `update_commands.rs`, so they can run in their own sessions while phase 1 is worked on.

---

## Phase 1: Updater

### Decision: how releases are signed

**Recommended: a signed checksum list, verified with an embedded public key.**

- The release workflow writes `SHA256SUMS` (one line per release file) and signs it with **minisign** into `SHA256SUMS.minisig`. The secret key lives only in GitHub secrets.
- The app embeds the minisign **public** key (`update_signing_key.pub`, `include_str!`). It downloads `SHA256SUMS` + `.minisig`, checks the signature, then checks the downloaded file's SHA-256 against the list. Any failure: delete the download, do not install, show why.
- Crates: `minisign-verify` (pure Rust, MIT, no deps) and `sha2` (already in `Cargo.lock`). Both pass `deny.toml`'s license list.

Why not a bare `SHA256SUMS` alone: it comes from the same release, so whoever can replace a release file can replace the checksum too. It only catches corruption. The signature is what protects against a compromised account or release.

Why not `tauri-plugin-updater`: it covers the NSIS installer and the AppImage, but not the portable launcher (`portable_launcher` + `CS_PORTABLE_EXE`), which would then need its own path anyway. It also uses minisign underneath, so the trust model is the same; switching later is still possible.

**Needs the owner, once (cannot be done by an agent):**
1. `minisign -G -p update_signing_key.pub -s update_signing_key.key` on your own PC.
2. Add GitHub secrets `MINISIGN_SECRET_KEY` (file contents) and `MINISIGN_PASSWORD`.
3. Keep an offline backup of the secret key. Losing it means shipping a release that changes the embedded key, which every installed copy will reject: users would have to update by hand once.

**Rollout note:** v0.4.2 and older do not verify, so they will still install whatever the next release contains. Protection starts for users once they are on the first signed version.

### U1. Split `update_commands.rs` (≤ 300 lines each, no behaviour change)

**Status: done** (with U2). The `UpdateError` variants for U4 (`NoAsset`, `Signature`, ...) are added with U4, not before: unused variants fail clippy. The Unix-only code (`start_install_helper` for `sh`, `make_executable`) is not compiled on Windows; C2 covers it.

| New file | Holds |
|---|---|
| `update_version.rs` | `clean_version`, `parse_version`, `is_version_newer` + tests |
| `update_assets.rs` | `GitHubRelease`, `GitHubAsset`, `select_*_asset` + tests (both platforms, see U5) |
| `update_download.rs` | HTTP agent, `stream_to_file`, progress events |
| `update_install.rs` | `target_executable_path`, `check_target_writable`, the Windows/Unix helper script builders |
| `update_error.rs` | `UpdateError` (U2) |
| `update_commands.rs` | the two `#[tauri::command]`s only, mapping `UpdateError` to `String` at the boundary |

Rename one-letter names (`e` → `error`, `a` → `asset`, `l`/`c` → `latest`/`current`, `p`/`d` → `exe`/`folder`).
Done when: `cargo clippy -p desktop_tauri --all-targets -- -D warnings` and the existing tests pass, no file over 300 lines.

### U2. Typed errors

`UpdateError` (thiserror): `Check(reason)`, `NoAsset`, `Download { reason }`, `Disk { path, source }`, `ReadOnlyTarget(PathBuf)`, `Signature(reason)`, `ChecksumMismatch { file }`, `MissingChecksums`, `Spawn(reason)`. Internal functions return `Result<_, UpdateError>`; only the commands turn it into a `String` for the page.

### U3. Fix version comparison (downgrade risk)

**Status: done.** Tags are read with the `semver` crate; build metadata (`+...`) does not count; an incomplete tag such as `v0.5` is now unreadable too (all release tags are `x.y.z`). An unreadable tag is logged (`warn`) and not offered; the page then says the running version is the latest, which is wrong in that case but safe.

- `is_version_newer` returns `false` when either side does not parse (log a `warn` with the tag). Never "different = newer".
- Treat a pre-release suffix (`0.5.0-rc1`) as older than the plain version and do not offer it unless the current version is also a pre-release.
- Tests: unparseable tag, `v0.4.1` while on `0.4.2` (no), `0.4.2-rc1` while on `0.4.2` (no), `0.4.10` vs `0.4.9` (yes).

### U4. Verify before installing

**Status: done (app side).** `update_verify.rs`, `update_folder.rs`, `update_prepare.rs`; `apply_update` takes no address. Crates: `minisign-verify` (no dependencies), `sha2`; tests sign with `minisign` (dev only) and a key made per test, so no test key is checked in. `update_signing_key.pub` holds the owner's key `D09F1A068C469C6B` (added after U4; a test checks the file decodes). Not verified: a real end-to-end install (needs a signed release, U5).

- `apply_update` no longer takes a URL. The page calls `apply_update()`; the backend fetches the latest release again, picks the asset itself (`update_assets.rs`) and also finds `SHA256SUMS` and `SHA256SUMS.minisig` in the same release. (Fixes 1a. Update `api.ts`, `UpdateCard.tsx`.)
- New `update_verify.rs`: `verify_checksums(sums, sig, public_key) -> Result<Checksums, UpdateError>` and `verify_file(path, expected_sha256)`. Hash while streaming in `stream_to_file` (no second read).
- Download into a fresh folder under the app's own data folder (`<data>/updates/<random>/`), not the shared temp folder; write the helper script there too (fixes 1b). Remove the folder on any failure.
- A release without `SHA256SUMS` / `.minisig` is refused with "this release is not signed".
- Log: version, asset name, "signature ok" / the failure kind. Never the URL's query string.
- Tests (T1): good signature + matching hash passes; tampered file, tampered `SHA256SUMS`, wrong key, missing entry, missing `.minisig` each fail with their own error and leave no file behind. Use a test key pair checked into `tests/fixtures/` (test-only, never the release key).

### U5. Release workflow and docs

**Status: done.** `scripts/sign_release.sh` (sums, sign, check against the app's key) runs in `release.yml` `draft-release`; the hand-run build scripts print how to sign. Tested locally with minisign 0.12 and a throwaway key: unsigned, signed (CRLF key text), wrong key, wrong password, empty folder, and the app's verifier on the result (`verify_checksums_sign_release_script_output_is_accepted`, ignored test). Not verified: the workflow on GitHub (needs a tag; minisign comes from Ubuntu's package). Asset tests for both platforms were done in U1.

- `release.yml` `draft-release` job: after download, `sha256sum * > SHA256SUMS`, sign with minisign from the secrets, upload both with the other files. Fail the job if the secret is missing.
- Same for `scripts/build_release.ps1` / `.sh` when run by hand: write `SHA256SUMS`, sign only if a key path is given, otherwise print that the release is unsigned.
- `docs/RELEASING.md`: the key, the two secrets, "the app refuses unsigned releases", and the lost-key procedure.
- Asset tests: run both the Windows and the Linux selection on every OS (call `select_windows_asset` / `select_linux_asset` directly instead of `select_platform_asset` behind `cfg!`).

### U6. Unix helper script

Quote paths with a proper shell-quoting function (a path with `"` or `$` currently breaks the script), and the same for the `.bat` (`%`, `^`, `&`). Unit-test the generated text for both. Covered by CI on Linux after C2.

---

## Phase 2: CI

### C1. `cargo deny` and `cargo audit` in CI
New job `supply-chain` on `ubuntu-latest`: `EmbarkStudios/cargo-deny-action@v2` (uses `deny.toml`) and `cargo audit` via `taiki-e/install-action`. Also run it on a weekly `schedule:` so a new advisory shows up without a push.

### C2. Linux Rust job
New job `rust-linux` on `ubuntu-24.04`: install the Tauri/WebKitGTK build packages (reuse the list in `scripts/build_release.sh --install-deps`), then `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. This compiles the `#[cfg(not(windows))]` updater path and runs U4/U6 tests on Unix. Start with `cargo test -p desktop_tauri -p app_services` if the whole workspace is too slow, then widen.

### C3. File-length check
`scripts/check_file_lengths.py` (or a small `xtask`): fails on any `*.rs` outside `tests/` and `*_tests.rs` over 300 lines, prints the list. Add it to `ci.yml` and to the `justfile` `gates` target. Turn it on after R1, so it starts green.

### C4. Keep `gates` and CI the same list
Comment at the top of `ci.yml` and `justfile`: each lists the other; `RULES.md` §8 names both.

---

## Phase 3: Rule breaks

### R1. Split the five other oversize files
One file per responsibility, no behaviour change, tests unchanged:
- `browse_commands.rs` (369): TMDB request building / response mapping / the command.
- `cloudflare_fetch.rs` (366): the exchange-file loop / the page `fetch` call and result parsing.
- `settings_service.rs` (340): move groups of typed getters/setters into their own files, as `stream_proxy_setting.rs` already does.
- `cleanup_queue.rs` (333): queue mechanics / scheduling (start delay, pause while playing). It shares state between threads: keep its race tests passing unchanged (RULES.md §race tests).
- `playback_service.rs` (305): drops under 300 with R2; if not, move progress saving into its own file.

### R2. Remove the stray re-export
Delete `#[allow(unused_imports)] pub use crate::playback_types::{...}` from `playback_service.rs:10-11`; import only what the file uses (`StreamSelection`, ...) with a plain `use`. `lib.rs:39` already exports them.

### R3. Remove `AppState::write`
No caller (`app_state.rs:43`). Delete it and its `allow`.

---

## Phase 4: Workspace and docs

### D1. Placeholder crates
**Recommended:** delete `network_client`, `html_parsing`, `extractors` and their workspace entries; keep one line in `RULES.md`'s layout ("native extractors: planned, see spike_findings.md"). Re-create when there is real code. Alternative: keep them and accept the noise.

### D2. Make `AGENTS.md` honest
- Drop "verified" from the intro and the section labels; say instead "kept up to date by hand; the code wins when they differ".
- Fix the command count and table (64, not 57).

### D3. Drift guard for the part that drifts most
A test in `desktop_tauri` (`docs_drift_tests.rs`): reads `main.rs`'s `generate_handler!` list and the command table in `AGENTS.md`, fails when a command is missing from either. Cheap, catches the most common drift. (Paths and symbols elsewhere stay unchecked; D2 says so.)

---

## Done when

- The app refuses a release whose file or `SHA256SUMS` was changed after signing (U4 tests), and the first signed release installs end to end on Windows (installer and portable) and on one Linux AppImage.
- CI runs fmt, clippy, tests on Windows and Linux, deny, audit, and the file-length check, all green.
- No non-test `.rs` file over 300 lines; no `allow` without a written reason.
- `AGENTS.md` matches the command list (D3 green).
