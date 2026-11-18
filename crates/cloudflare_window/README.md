# cloudflare_window

`cloudstream-verify`, the browser window in which a site's Cloudflare check is passed. A separate
process (tao + wry: WebView2 on Windows, the system's WebKitGTK 4.1 on Linux) so the WebView's event
loop never runs inside the app's Slint loop. Started by `cloudflare_bridge::VerifyHelper`.

- `--exchange <dir> --profile <dir>`; commands on stdin, statuses on stdout, exits when stdin closes.
- One window per site, opened on the site's front page (never the request's own address); downloads
  are refused; the browser profile keeps cookies, so a passed site stays passed across restarts.
- Passing: `cf_clearance` cookie + the browser's User-Agent, then one probe request from inside the
  page (no challenge = passed). If the site redirected elsewhere after the check, the cookie is used.
- After passing the window is hidden and runs the sidecar's browser fetches (`fetch_script`).

| Part | Responsibility |
|---|---|
| `fetch_script` (lib) | the page scripts that run a request and read its answer back (ported from the Tauri shell) |
| `verify_progress` (lib) | the pure next-step rule (wait / probe / pass on cookie / give up) |
| `site_window`, `page_fetches`, `browser_view` | one site's window, its running requests, building the view |
| `helper_app`, `input_threads`, `outbox` | the event loop, stdin and fetch-file threads, answers |

Test: `cargo test -p cloudflare_window` (T1). The window itself was checked by hand against a real
Cloudflare-protected site (managed challenge passed, cookies and User-Agent handed back).
