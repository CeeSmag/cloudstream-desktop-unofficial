# cloudflare_window

`cloudstream-verify`, the browser window in which a site's Cloudflare check is passed. A separate
process (tao + wry: WebView2 on Windows, the system's WebKitGTK 4.1 on Linux) so the WebView's event
loop never runs inside the app's Slint loop. Started by `cloudflare_bridge::VerifyHelper`.

- `--exchange <dir> --profile <dir>`; commands on stdin, statuses on stdout, exits when stdin closes.
- One window per site. Loads the sidecar's challenged URL when it is http(s) on that host (or a
  subdomain) and not a media/segment path; otherwise the site's front page (`verify_open_url`).
  Downloads are refused. Starts hidden (~3 s) so a Managed challenge can auto-pass; then shows if
  still waiting. The browser profile keeps cookies, so a passed site stays passed across restarts.
- Passing: `cf_clearance` cookie + the browser's User-Agent, then one probe request from inside the
  page (no challenge = passed). If the site redirected elsewhere after the check, the cookie is used
  (no front-page reload — bad-TLS redirect hosts would loop).
- After passing the window is hidden and runs the sidecar's browser fetches (`fetch_script`).
- Linux: the verify WebKit context ignores TLS errors so a self-signed redirect host does not freeze
  on WebKit's certificate page (`webkit2gtk`).

| Part | Responsibility |
|---|---|
| `fetch_script` (lib) | the page scripts (`script_text`) that run a request and read its answer back |
| `verify_progress` (lib) | the pure next-step rule (wait / probe / pass on cookie / give up) |
| `verify_open_url` (lib) | which address to load (`verify_open_url`, `short_address`) |
| `site_window`, `site_window_check`, `page_fetches`, `browser_view` | one site's window, its check tick, running requests, building the view |
| `helper_app`, `helper_events`, `input_threads`, `outbox` | the event loop, stdin and fetch-file threads, answers |

Test: `cargo test -p cloudflare_window` (T1). The window itself was checked by hand against a real
Cloudflare-protected site (managed challenge passed, cookies and User-Agent handed back).
