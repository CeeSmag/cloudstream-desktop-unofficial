# cloudflare_window

`cloudstream-verify` hosts the viewer's Cloudflare check in a separate process. It uses
WebView2 on Windows and WebKitGTK 4.1 (WebKit 2.40 or newer) on Linux. The app starts it
with `--exchange <dir> --profile <dir>` and communicates over stdin and stdout.

- Each check owns its original request and exact challenged GET. It starts concealed.
  The viewer may open it only while that request has a confirmed top-level challenge.
- Native navigation hides the previous document before loading the next one. The helper
  refuses downloads and popups and keeps ordinary destinations concealed.
- The helper observes the challenge origin's own response and takes a scoped cookie
  snapshot at that point. When passage is confirmed, it hides the window, delivers the
  result, and retires the request. A later close or duplicate click cannot change the result.
- A verified same-origin document can stay hidden for sidecar browser fetches. An
  off-origin redirect cannot become that document. TLS uses the browser's default validation.
- On Windows the app grants foreground permission to its child. The helper uses that grant
  at most once per presentation action, after the matching challenge is eligible to show.

Run `cargo test -p cloudflare_window` for the decision and race tests. Run
`python scripts/check_verify_window.py <helper>` for native browser and window checks.
On Linux, use an X11 session with `xdotool` and a window manager.
