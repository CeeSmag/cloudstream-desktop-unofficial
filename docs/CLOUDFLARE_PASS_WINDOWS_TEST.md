# Saved Cloudflare passes: manual test on Windows

What this checks: the parts of saved passes that were compiled for Windows but never run there
(`docs/CLOUDFLARE_PASS_PLAN.md`, "Not verified yet"). Above all, WebView2's cookie domain field: a domain
cookie must arrive as `.site.com`, as in WebKit. If it does not, every pass is stored host-only, and sharing
across subdomains silently stops working.

Takes about 20 minutes. You need a provider whose site shows a Cloudflare check.

> **The pass files and the check browser's profile hold live credentials.** When you send results, send
> log lines and file *names*, never the content of a `cloudflare_passes\*.json` file and never a
> `cookie=` / `"value":` line.

## 0. Build with a fresh sidecar

`scripts\build_release.ps1` packages whatever sidecar `scripts\setup_sidecar.ps1` last built; it does not
check whether that build is current. The saved-pass work changed the sidecar, so rebuild it first:

```powershell
git pull
powershell -File scripts\setup_sidecar.ps1
powershell -File scripts\build_release.ps1 -Kind portable
```

Check that the sidecar is the new one (it must print a line):

```powershell
tar -tf C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar\lib\plugin-runtime.jar | Select-String CloudflareExchangeFormat
```

An old sidecar with the new app is the one combination that must not ship: a stale saved pass would wrongly
mark the site browser-only for the session.

## 1. Start clean, with debug logs

1. Close the app. Rename `%LOCALAPPDATA%\CloudStreamRs\cloudflare_passes` if it exists (for the portable app:
   `data\cloudflare_passes` beside the exe).
2. Start the app: **Settings → General → Write detailed debug logs** on.
3. Logs: `%LOCALAPPDATA%\CloudStreamRs\logs\debug\cloudflare-logs.jsonl`.

A helper for the steps below (PowerShell):

```powershell
$data = "$env:LOCALAPPDATA\CloudStreamRs"
function cf { param($pattern) Select-String -Path "$data\logs\debug\cloudflare-logs.jsonl" -Pattern $pattern | Select-Object -Last 10 }
```

## 2. First check: the pass is kept

1. Play a title from the protected provider and pass the check (press Verify if asked).
2. Expect in the log:
   - `cf "kept the check's Cloudflare pass"`: one line whose `"scope"` field is `".something.com"` (a
     leading dot) or `"www.something.com"` (host-only). **Write down which one.**
   - `cf "the app verified"`: the sidecar's line, ending `from the browser`.
3. Expect a file: `dir $data\cloudflare_passes`, named `domain-<site>.json` or `host-<site>.json`.

**The WebView2 question:** open the site's cookies in Edge (F12 → Application → Cookies, same site). If Edge
shows the `cf_clearance` *Domain* with a leading dot but our file is `host-…json`, the WebView2 domain field
is read wrongly: report it with the file name and the Edge Domain value (not the cookie value).

## 3. Restart: answered from the saved pass, no window

1. Close the app completely, start it again, play a title from the same provider.
2. Expect:
   - `cf "answered from a saved Cloudflare pass"`, with `"valid_for_secs"` above 0.
   - `cf "from a saved pass"` (the sidecar side).
   - **No** check window, and no new `verification helper started` line after the restart:
     `cf "verification helper started"`.
3. If the site has a second host (an `api.`, `cdn.` or `www.` sibling) and step 2 showed a leading-dot scope,
   a request to the sibling should also be answered from the pass.

## 4. A refused pass is replaced, not looped

Easiest way to make the site refuse the pass: change your public IP (switch to a phone hotspot or a VPN),
then restart the app and play again. Expect, in this order:

- `cf "answered from a saved Cloudflare pass"`
- `cf "refused the saved pass"` (sidecar)
- `cf "is retired and the site is checked again"` (app)
- a new check (hidden window or a Verify prompt), then a new `kept the check's Cloudflare pass`.

Then: no further `answered from a saved Cloudflare pass` for the *old* pass id, the site still plays, and
`cf "does not challenge the browser"` does **not** appear (a refused *saved* pass must not make the site
browser-only).

## 5. Plugins cannot read the folder

The plugins' AppContainer (`CloudStream.PluginSidecar.v1`) must have no access to the pass folder:

```powershell
icacls "$data\cloudflare_passes"
icacls "$data\sidecar"
```

`sidecar` lists an entry for the AppContainer SID (`S-1-15-2-…`); `cloudflare_passes` must not.

## 6. Two app instances (optional)

Start the portable exe twice on the same data folder, play from the protected provider in both. Expect no
`stayed locked` and no `removing a saved Cloudflare pass that cannot be read` lines: `cf "stayed locked|cannot be read"`.

## Report

For each step: pass / fail, the matching log lines (cookie-free as above), the `cloudflare_passes` file
names, and for step 2 the scope kind (`.domain` or host-only) and Edge's Domain value. Add the Windows
version and whether it was the installer or the portable exe.
