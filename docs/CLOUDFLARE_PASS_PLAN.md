# Cloudflare passes: keep them across restarts, share them across a site

Status: **not implemented** (findings of Nov 23, for later). Nothing here changes the app yet.

A "pass" is what passing a Cloudflare check gives: the `cf_clearance` cookie (plus the other cookies the
check window read) and the browser identity (user agent) it was issued to.

## How it works today (from the code)

| What | Kept where | Scope |
|---|---|---|
| The viewer's permission (Verify) | setting `cloudflare_allowed_hosts` (`cloudflare_bridge/src/allowed_hosts.rs`) | the site and its subdomains, until removed |
| The pass | the sidecar's memory (`CloudflareSolver.kt`, `clearances`, keyed by `Origin`) | one exact origin (scheme, host, port) |
| Requests through the browser window | the check window (`cloudflare_window`) | one exact origin (same-origin rule) |

- The sidecar does **not** track expiry: it reuses a pass until the site challenges again, then drops it
  and asks the app for a new check (`intercept`, "A clearance obtained after this request began…").
- A pass lives only in **one plugin process**: every `.cs3` has its own JVM (`PluginWorkerPool`), so two
  plugins using the same site verify it separately, and a stopped or parked plugin loses its passes.
- The check window's browser profile (`<data>/cloudflare_browser`) is on disk, so its own cookies survive
  a restart; whether a re-check then passes without a puzzle was **not** confirmed in a real session.

## How long a pass lasts

Set by each site's owner in Cloudflare ("Challenge Passage"): **30 minutes by default**, configurable from
about 5 minutes to 1 year (from Cloudflare's documentation, not re-checked). The real value is the
`Expires` of the site's `cf_clearance` cookie. It ends earlier when the viewer's **IP address** changes,
the **user agent** changes, or the site raises its protection (e.g. "Under Attack" mode).

## 1. Keep passes on disk until they expire

Goal: after a relaunch, a parked plugin coming back, or a second plugin on the same site, an unexpired
pass is used at once: no check window (each costs a browser process, about 60-150 MiB).

1. **Read the expiry.** The check window reads only cookie names and values (`webkit_cookies.rs` on Linux,
   `read_cookies` in `webview2_browser.rs` on Windows). Both browsers know each cookie's expiry: read it too.
2. **Carry it.** The result file has `cookie=name=value` lines (`cloudflare_bridge/src/exchange.rs`); add the
   expiry per cookie in a backward-compatible way.
3. **Cache in the app, not the sidecar.** `<data>/sidecar` is writable by every plugin's sandbox (untrusted
   code), so a cache there would let any plugin read every site's pass. Keep it in a folder of its own owned
   by `cloudflare_bridge`, outside the sandbox: site (origin), cookies, each cookie's expiry, user agent,
   time obtained. Atomic writes (temp file + rename), entries past their earliest cookie expiry ignored and
   pruned.
4. **Answer from the cache.** When the sidecar asks for a check, the bridge answers at once with an
   unexpired pass instead of opening the window. The sidecar needs no change: it already takes whatever
   the app answers.

Must handle:

- **A pass rejected before it expires** (IP or user agent changed, site re-challenges): the sidecar asks
  again right after receiving it. The bridge must recognise the pass it just handed out failed, drop it and
  open the window, never hand the same pass out in a loop.
- **The user agent must go with the pass** (Cloudflare ties the pass to it); the exchange already carries it.
- **Browser-only sites** (`transport=browser`: no cookie, the sidecar's own HTTP client is refused) cannot be
  cached: they need the live window.
- **Several plugin processes at once** read and update it: race tests in the same change (RULES.md, AGENTS.md
  Recipe 6), plus a test for the rejected-pass loop.
- Never log cookie values (only names and domains, as today).

## 2. Share a pass across a site's subdomains

Passes cannot be shared between **different** sites: Cloudflare accepts a pass only on the site that issued
it, and handing one site's pass to another server would leak it (the sidecar deliberately attaches a pass
only to its own origin, even across a redirect: `clearanceScope`).

Across one site's **subdomains** it is possible: Cloudflare usually sets `cf_clearance` for the whole domain
(`Domain=.site.com`), so one pass is valid on `www.`, `api.` and so on, but the sidecar keys passes by exact
origin and checks each host separately.

- Key a pass by the domain the cookie itself names (its `Domain` attribute), never by a guess: a pass set
  for a single host stays single-host. This needs the cookie's domain carried with it (as for the expiry).
- Browser-only sites cannot share (the window can only fetch from its own origin).
- Worth it only if plugins really hit several subdomains of one protected site: check
  `logs/debug/cloudflare-logs.jsonl` for checks of sibling hosts first.

Changes for this part would be in `CloudflareSolver.kt` (with `CloudflareSolverTest.kt`), or in the bridge's
cache from part 1 if both are done together.
