# cloudflare_bridge

Routes Cloudflare verification between the Kotlin sidecar, Slint banner, and native
`cloudstream-verify` process. There is no GUI code in this crate.

- `request-<id>.txt` names the effective URL and method of the response that triggered
  verification. `result-<id>.txt` carries the outcome and browser identity on success:
  cookies, or `transport=browser` when the browser was not challenged and has no cookies to give.
- State is shared by what owns it. Permission belongs to a host. The prompt, the check and the browser
  document belong to an **origin** (`VerifyRequest::origin_key`: scheme, host, port). Only the result
  file, the exact address to open and the terminal status belong to one request.
- The first request of an origin owns the prompt (`prompt_owners.rs`): its events reach the banner and its
  exact address is what the viewer opens. Later requests of that origin are quiet: they are still sent to
  the helper (which checks the origin once and answers them all), but raise no events. For a site the viewer
  has not allowed, a later request is answered "ask again next time" at once instead of holding a plugin
  thread. Dismissing the prompt retires the whole group; if the provider withdraws the owner, the oldest
  waiting request takes the prompt over.
- Verify, Open, Dismiss, and terminal statuses refer to an individual request ID. A pending prompt
  retires when the provider withdraws it.
- Try again starts a fresh check of the failed request's exact destination. If the
  original sidecar call has ended, the retry reports its result to the app.
- `VerifyHelper` starts the helper on demand. The watcher tracks which child owns
  an active request. If it exits after publishing the answer, the watcher recovers
  the status from that answer. Only viewer-initiated presentation can request Windows
  foreground permission.
- The `fetch-<id>.txt` exchange serves browser requests from a verified same-origin document.

Run `cargo test -p cloudflare_bridge` for exchange, protocol, prompt-owner and watcher tests.
