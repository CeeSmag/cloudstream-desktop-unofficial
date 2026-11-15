# stream_proxy

A small local HTTP proxy for HLS streams. The player is given `http://127.0.0.1:<port>/s/<token>/<id>.<ext>`
instead of the stream's address; the proxy fetches every playlist, segment and key with the provider's headers
and referer, resolves relative addresses like the player would, and rewrites playlists to point back at itself.

- **Safe by construction:** loopback only, a random per-stream token, GET/HEAD only, `http`/`https` upstream only,
  cookies and credentials only to the host the stream started on, no real URL in the addresses the player logs.
- **Public API:** `StreamSpec` (url, headers, referer), `ProxyConfig` (limits and timeouts), `StreamProxy` (`start`, `open`,
  `close`, `shutdown`, `port`).
- **Why (checked with the real libmpv):** libmpv asks for `Range: bytes=0-` on every request, so a host that refuses
  ranges cannot be played directly; through the proxy it plays. Headers on every piece and segments named `.jpg` are
  *not* a problem for libmpv, so they are no reason to proxy. A playlist is always read whole and the player's range
  is answered over the rewritten text.
- **No new dependencies:** std sockets and threads, `ureq` for upstream requests.
- **Not here:** DASH to HLS, CENC decryption (HLS AES-128 needs nothing: the player decrypts, the proxy only
  carries the key request with the right headers).
- **Test:** `cargo test -p stream_proxy` (fake origin servers on real sockets, race tests for sessions, the gate and
  shutdown). With the real libmpv: `CS_LIBMPV_PATH=... CS_TEST_TS_SEGMENT=<any .ts file> cargo test -p app_shell --lib -- --ignored hls`.
