# cloudflare_bridge

The Cloudflare "verify you are human" exchange between the sidecar, the app and the verification
helper (`cloudflare_window`). No GUI code here.

| Part | Responsibility |
|---|---|
| `exchange` | `request-<id>.txt` (sidecar asks) / `result-<id>.txt` (answer: cookies + User-Agent, declined, or "ask again") |
| `fetch_exchange` | `fetch-<id>.txt` / `fetchresult-<id>.txt`: a request the sidecar wants made inside the verified window |
| `AllowedHosts` | sites the viewer pressed Verify for (and their subdomains); shared by the UI and the watcher |
| `helper_protocol` | the tab-separated lines on the helper's stdin (`verify`, `quit`) and stdout (`open`, `passed`, `failed`) |
| `VerifyHelper` | starts `cloudstream-verify` on demand, restarts it once if its pipe broke, ends it on drop |
| `spawn_watcher` | answers every request at once: not allowed → "ask again" + `NeedsPermission` for the UI's banner; allowed → the helper; browser fetches with no helper → "no window" (instead of a 90 s wait) |

Why: the former Slint UI opened each request's own address in the system browser for every request
of an allowed site (often a video, so the browser downloaded it again and again) and could never hand
cookies back.

Test: `cargo test -p cloudflare_bridge` (T1 formats and gate, race test of the gate, T2 watcher with a
scripted helper process on a real folder).
