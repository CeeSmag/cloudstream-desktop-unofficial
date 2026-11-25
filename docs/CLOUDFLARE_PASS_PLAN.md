# Saved Cloudflare Passes

Status: **implemented & verified on Linux and Windows** (Nov 23–24).

A "pass" is the verified `cf_clearance` cookie (plus accompanying cookies) and browser `User-Agent` obtained from `cloudstream-verify`. Passes persist across restarts and share across subdomains strictly according to the cookie's RFC 6265 `Domain`/`Path`/`Secure` attributes.

## Scope & Storage Map

| State | Where | Scope |
|---|---|---|
| Viewer permission (`Verify`) | Setting `cloudflare_allowed_hosts` (`allowed_hosts.rs`) | Site + subdomains, until removed |
| Saved pass (app) | `<data>/cloudflare_passes/<scope>.json` (`DiskPassBook`) | Exact hosts reached by `cf_clearance`'s `Domain` |
| In-memory clearance (plugin) | Sidecar `CloudflareSolver.kt` (`clearances`) | Exact `Origin` |
| Browser-transport fetches | `cloudflare_window` (`cloudstream-verify`) | Exact `Origin` |

## How It Works

1. **Attribute capture:** Both engines read full cookie attributes (`webkit_cookies.rs` on Linux, `webview2_cookies.rs` on Windows; shared validation in `browser_cookies.rs` and `PassCookie::applies_to`).
2. **Private hand-off:** `cloudstream-verify` sends `passed\t<host>\t<id>\t<json>` over its private stdout pipe (result files add `validUntil=<epoch ms>`). Cookie values are redacted in `CookieValue`'s `Debug`, malformed status lines log only the verb, and serde errors omit input text.
3. **Persistence (`watch_passes.rs` -> `DiskPassBook`):** Governed by the earliest applying non-empty `cf_clearance` with `Expires` (capped at 400 days minus 60 s margin; session clearances are not saved). Accompanying session cookies are served only within the same app run.
4. **Serving (`pass_policy::decide`):** Answers immediately (`source=saved`, `pass=<id>`, `validUntil`) for ordinary requests whose host is still allowed and covered by a fresh pass — bypassed for viewer retries, `need=browser`, or `rejected=`.
5. **Retirement & loop guard:** When a site refuses a saved pass, the sidecar (`CloudflareExchangeFormat.kt`) sends `rejected=<id>` once per origin (never marking the origin browser-only for a refused *saved* pass; only a refused *fresh* pass does that). The app permanently retires the clearance until its expiry and re-verifies in the browser. `reask_guard` caps hand-outs at 4 per origin per 10 s.

## Storage & Isolation

- `<data>/cloudflare_passes` lives **beside** `<data>/sidecar`, never inside it (untrusted plugin sandboxes only receive access to `<data>/sidecar`: Landlock on Linux, AppContainer `CloudStream.PluginSidecar.v1` on Windows).
- Unix enforces `0700` dir / `0600` files; Windows inherits the user profile ACL with no AppContainer SID.
- One versioned JSON file per scope (`domain-site.test.json`, `host-www.site.test.json`), max 64 KiB, max 256 scopes, atomic temp+rename writes. Corrupt/oversized/misnamed files are logged by name and removed.
- **Multi-instance safety:** Writes and reads coordinate via `.lock` (2 s timeout; exclusive lock when writable to avoid Windows NTFS `DELETE_PENDING` read errors and reader-overlap writer starvation, shared lock fallback on read-only lock files). The file lock is polled (no timed wait in safe Rust), which is unfair, so books in one process first queue per folder in `FolderGate` (ticket order, no time limit); the 2 s limit then only applies while another app instance holds the lock.
- **Windows sidecar guard:** `scripts/setup_sidecar.ps1` writes `.sidecar-sources.stamp` and `scripts/build_release.ps1` verifies source freshness and `CloudflareExchangeFormat.class` in `plugin-runtime.jar` via `scripts/sidecar_stamp.py` so stale sidecars cannot ship.

## Verification & Mutation Testing

- **Automated Rust & Python tests (Windows & Linux):**
  - `cargo test -p cloudflare_bridge -p cloudflare_window -p extension_runtime`: unit/proptest rules, disk faults, contract suite, cross-platform `t2_saved_passes` + `t2_watch` via `fake_verify_helper`, Windows AppContainer `icacls` isolation, and multi-instance / single-scope NTFS `DELETE_PENDING` race tests.
  - `python -m unittest discover -s scripts -p "sidecar_stamp_test.py"`: Windows sidecar freshness & JAR contract tests.
  - `python scripts/check_verify_window.py <cloudstream-verify>`: native WebView2 (Windows) / WebKitGTK (Linux) checks including `BrowserClear` and domain/host-only/expiry/session cookies (`*.localhost`).
  - `cargo test -p cloudflare_bridge --test t4_windows_saved_passes -- --ignored`: live Windows WebView2 end-to-end test covering first check, restart without session cookies, sibling host reuse without starting the helper, `rejected=<id>` retirement + replacement, and zero cookie leaks in logs.
  - Kotlin sidecar (`scripts/test_sidecar.ps1`): `CloudflareSolverTest` & `CloudflareExchangeFormatTest` pass.
- **Mutation testing:**
  - `cloudflare_bridge` (Linux): 543 mutants, 0 new survivors in compiled code; (Windows `disk_pass_book.rs`): 68 mutants + 10-mutant re-verify — both Windows `#[cfg(not(unix))]` paths caught, `ensure_lock_file` & `is_shared_fallback_error` caught, 2 expected infinite-loop timeouts on `may_wait_longer`, 2 uncompiled `#[cfg(unix)]` twins, 1 Windows-only OS-equivalent match-guard survivor in `open_read_lock` (Win32 maps both dirs and read-only files to `PermissionDenied`, unlike POSIX `EISDIR` vs `EACCES`).
  - `cloudflare_window` (Windows `webview2_cookies.rs`, `browser_cookies.rs`, `outbox.rs`, `verification.rs`): 83 mutants, 71 caught, 10 unviable, 2 missed (only the 1-line OS/COM wrappers `Outbox::status` and `webview2_cookies::read_cookies`, both exercised by `check_verify_window.py` & T4).
  - Kotlin PITest (`CloudflareSolver*`, `CloudflareExchangeFormat*`): 200 mutants, 180 killed (94% test strength).
- **Live Linux session (Nov 23):** verified domain passes (`.vcloud.my`, `.cinevood.loan`), 0.2 s relaunch reuse without window, VPN IP-change refusal + retirement + fresh re-check, `0700`/`0600` permissions, and zero leaked cookie values.

## Optional Manual Smoke (Windows, Real Internet)

To double-check against a live third-party Cloudflare site on Windows: enable **Settings → General → Write detailed debug logs**, pass a challenge (confirm `kept the check's Cloudflare pass` in `%LOCALAPPDATA%\CloudStreamRs\logs\debug\cloudflare-logs.jsonl` and `domain-*.json` in `cloudflare_passes`), restart to confirm `answered from a saved Cloudflare pass` with no helper start, then switch VPN/IP to confirm `refused the saved pass` -> `is retired and the site is checked again` (never sharing raw `cloudflare_passes/*.json` contents or `cookie=` lines).
