# cloudflare_bridge

Routes Cloudflare verification between the Kotlin sidecar, Slint banner, and native
`cloudstream-verify` process. There is no GUI code in this crate.

- `request-<id>.txt` names the effective URL and method of the response that triggered
  verification. `result-<id>.txt` carries the outcome and browser identity on success.
- Host permission is shared. Verify, Open, Dismiss, and terminal statuses refer to an
  individual request ID. A pending prompt retires when the provider withdraws it.
- Try again starts a fresh check of the failed request's exact destination. If the
  original sidecar call has ended, the retry reports its result to the app.
- `VerifyHelper` starts the helper on demand. The watcher tracks which child owns
  an active request. If it exits after publishing the answer, the watcher recovers
  the status from that answer. Only viewer-initiated presentation can request Windows
  foreground permission.
- The `fetch-<id>.txt` exchange serves browser requests from a verified same-origin document.

Run `cargo test -p cloudflare_bridge` for exchange, protocol, and watcher tests.
