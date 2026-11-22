# cloudflare_window

`cloudstream-verify` hosts the viewer's Cloudflare check in a separate process. It uses
WebView2 on Windows and WebKitGTK 4.1 (WebKit 2.40 or newer) on Linux. The app starts it
with `--exchange <dir> --profile <dir>` and communicates over stdin and stdout.

- One window checks one **origin** at a time (`waiters.rs`). The request whose exact challenged GET
  opened it is the owner; requests for the same origin that arrive while it is pending wait with it,
  and every request gets exactly one answer file and one status when the check ends. Retiring one request
  (`cancel`) leaves the check running for the others. A viewer's retry always asks for a check of its own.
  The window starts concealed. The viewer may open it only while the owner has a confirmed top-level
  challenge.
- Native navigation hides the previous document before loading the next one. The helper
  refuses downloads and popups and keeps ordinary destinations concealed.
- The helper observes the challenge origin's own response and takes a scoped cookie
  snapshot at that point. A passage needs a fresh `cf_clearance` after a challenge. If the browser was never
  challenged and lands on a same-origin 2xx document (only the sidecar's own connection is challenged), there
  are no cookies to hand over: the result says `transport=browser` and the sidecar makes that origin's
  requests through the browser document. A challenge followed by a missing or stale cookie is not that.
  When passage is confirmed, it hides the window, delivers the results, and retires the requests. A later
  close or duplicate click cannot change them.
- A verified same-origin document can stay hidden for sidecar browser fetches. An
  off-origin redirect cannot become that document (the sidecar then stops asking for that origin for a
  while, see `CloudflareSolver`). TLS uses the browser's default validation.
- The debug log of each main response names where a redirect leads and which cookies exist (names only).
- On Windows the app grants foreground permission to its child. The helper uses that grant
  at most once per presentation action, after the matching challenge is eligible to show.

Run `cargo test -p cloudflare_window` for the decision and race tests. Run
`python scripts/check_verify_window.py <helper>` for native browser and window checks.
On Linux, use an X11 session with `xdotool` and a window manager.
