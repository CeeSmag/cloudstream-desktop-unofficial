# Cloudflare passes: keep them across restarts, share them across a site

Status: **implemented** (Nov 23). Parts 1 and 2 below are built; the design notes say where the code differs
from the first plan and why. What is not verified yet is listed at the end.

A "pass" is what passing a Cloudflare check gives: the `cf_clearance` cookie (plus the other cookies the
check window read) and the browser identity (user agent) it was issued to.

## How it works now

| What | Kept where | Scope |
|---|---|---|
| The viewer's permission (Verify) | setting `cloudflare_allowed_hosts` (`cloudflare_bridge/src/allowed_hosts.rs`) | the site and its subdomains, until removed |
| The pass, for the app | `<data>/cloudflare_passes/<scope>.json` (`DiskPassBook`) | the hosts its `cf_clearance` cookie reaches (its own `Domain` attribute) |
| The pass, in a plugin | the sidecar's memory (`CloudflareSolver.kt`, `clearances`, keyed by `Origin`) | one exact origin |
| Requests through the browser window | the check window (`cloudflare_window`) | one exact origin (same-origin rule) |

1. **The window reads every cookie's attributes** (domain, path, Secure, HttpOnly, expiry) in both browsers
   (`webkit_cookies.rs`, `webview2_browser.rs`, shared rules in `browser_cookies.rs`). The scope rule is the
   browser's own (`PassCookie::applies_to`, RFC 6265 domain/path match), one copy used by window and app.
2. **The helper reports the pass to the app** on its private stdout: `passed\t<host>\t<id>\t<json>`. The
   result file alone could not carry it: the helper writes it and the sidecar deletes it once read, so the app
   never saw it. The result file only gains `validUntil=<epoch ms>`.
3. **The app keeps it** (`watch_passes.rs` → `SavedPass` → `PassBook`), only for a request it sent to the
   helper and for the host the helper names. A pass is governed by its earliest applying, non-empty
   `cf_clearance` with an expiry: it lives until that cookie's `Expires` (capped at 400 days, RFC 6265bis),
   minus a 60 s margin, and reaches exactly that cookie's hosts. A session clearance is not kept (nothing says
   how long the site honours it). Session cookies of a kept pass are handed out only in the run that saved
   them, as a browser forgets them on restart.
4. **The app answers from it** (`pass_policy::decide`, pure) before routing a request, when the request is an
   ordinary challenge (not a viewer's retry, no `need=browser`, no `rejected=`), a fresh pass reaches its
   address, and the pass's issuing site is still allowed. The answer is
   `ok=true, userAgent, source=saved, pass=<id>, validUntil, cookie=…`; the newest such pass wins.
5. **A refused pass is retired for good.** The sidecar names it in its next question (`rejected=<id>`); the app
   drops it, remembers its clearance until it would have expired (so a browser profile that still holds it
   cannot bring it back), and checks the site in the window. `reask_guard` is the safety net under that
   protocol: one pass goes to one origin at most 4 times per 10 s.

### Where the code differs from the first plan, and why

- **The sidecar needed a change** (the plan said it would not). It treated *every* refused pass as "this
  site does not accept our connection's cookies" and made the origin browser-only for the session; a stale
  saved pass (new IP, new browser) would have done that. Now `CloudflareExchangeFormat.kt` reads
  `source=saved`, a refused saved pass is recorded per origin and named once by whichever request asks the app
  next (concurrent requests share one fresh check), and only a refused *fresh* pass means browser-only. A
  browser-only origin asks with `need=browser`, so it is never handed a cookie pass. An ended pass
  (`validUntil`) is not sent at all.
- **Logs:** the helper's unparsed stdout used to be logged word for word; a damaged `passed` line would have
  logged cookie values. Status-looking lines that do not parse are now logged by their verb only, serde's
  messages (which can quote the input) are reduced to kind and position, and `CookieValue` prints
  `<redacted>` in every `Debug`.
- **Two app instances:** writes to the pass folder take an exclusive lock (`.lock`, 2 s limit), so one
  instance's rejection is never lost to the other's write.

## Storage and safety

- `<data>/cloudflare_passes` sits beside `<data>/sidecar`, never inside: the sidecar folder is the only part of
  the data folder an untrusted plugin's sandbox may read (Landlock, AppContainer). Folder `0700`, files `0600`
  on Unix; on Windows the folder inherits the user profile's ACL. A trusted plugin runs without the file
  system jail and could read it, as it could read the check browser's own profile.
- One versioned JSON file per scope (`domain-site.test.json`, `host-www.site.test.json`), written whole
  (unique temp + rename). A file that is damaged, too large (64 KiB), of another version or misnamed is
  removed and logged by name. At most 256 scopes (oldest passes evicted, rejection lists never).
- "Delete all saved data" removes the whole data folder, this one included.

## Tests

Rust (`cargo test -p cloudflare_bridge -p cloudflare_window`): pure rules (cookie scope incl. proptests,
`SavedPass`, scope rules, policy for every bypass reason, guard), the file schema, disk faults, one contract
suite run against both pass books, the watcher glue step by step, and T2 with a scripted helper (sibling host,
restart, host-only, refusal, browser-only, permission gone). Race tests: two pass books on one folder never
lose a rejection (fails 4/4 with the lock removed); eight plugins asking about three hosts at once each get
exactly one answer from exactly one source, and the guard limit holds. Kotlin (`CloudflareSolverTest`,
`CloudflareExchangeFormatTest`): saved passes refused/accepted, bounded refusals, ended passes, `need=browser`,
and twenty requests refused with one saved pass naming it once (failed before the per-origin record). Fuzz:
`fuzz/fuzz_targets/cloudflare_exchange.rs` and the stable corpus stress in `tests/bdd`.

### Mutation testing and coverage (Nov 23)

- `cargo mutants -p cloudflare_bridge`: 543 mutants; no new survivor in the code that existed before (its 47
  survivors are those of the baseline run); every survivor in the new code was killed by a test, except the
  `cfg(not(unix))` twins of `create_private_dir` / `write_private`, which Linux does not compile.
- `cloudflare_window` (`browser_cookies.rs`, `outbox.rs`, `verification.rs`): 80 mutants, 70 caught, 9
  unviable, 1 missed: `Outbox::status`, the one-line hand-over of the real stdout to the tested
  `send_status`.
- PITest on `CloudflareSolver*` / `CloudflareExchangeFormat*`: 76 killed; the 5 survivors are four mutations
  of inlined Kotlin stdlib code mapped past the end of `CloudflareExchangeFormat.kt` and `challengeLocks.clear`
  in `reset` (equivalent); 7 without coverage are generated property getters and the default clock.
- Line coverage of the new bridge files 97-100 % (`cloudflare_bridge` crate 92 %, threshold raised to 90 %).

## Not verified yet

- A real session: a relaunch answered from a saved pass with no window, and a pass refused after an IP change.
- WebView2's domain field for a domain cookie (assumed to start with `.`, as in WebKit and Chromium's
  cookie API); compiled and linted for Windows, not run there.
- Whether real plugins hit sibling subdomains of one protected site (part 2 costs nothing extra, since the
  scope comes from the cookie, but its benefit is unmeasured; `logs/debug/cloudflare-logs.jsonl` will show
  `answered from a saved Cloudflare pass` with the host).
