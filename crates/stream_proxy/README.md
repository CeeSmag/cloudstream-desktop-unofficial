# stream_proxy

A small local HTTP proxy for HLS streams. The player is given `http://127.0.0.1:<port>/s/<token>/<id>.<ext>`
instead of the stream's address; the proxy fetches every playlist, segment and key with the provider's headers
and referer, resolves relative addresses like the player would, and rewrites playlists to point back at itself.

- **Why:** hosts that want a Referer, cookies or a User-Agent on every piece (not only on the playlist), signed
  playlists, segments hidden behind `.jpg` names, redirects. Playing directly fails or stalls on those.
- **Safe by construction:** loopback only, a random per-stream token, GET/HEAD only, `http`/`https` upstream only,
  cookies and credentials only to the host the stream started on, no real URL in the addresses the player logs.
- **Public API:** `StreamSpec` (url, headers, referer), `StreamProxy` (`start`, `open`, `close`, `shutdown`).
- **No new dependencies:** std sockets and threads, `ureq` for upstream requests.
- **Not here:** DASH to HLS, CENC decryption (HLS AES-128 needs nothing: the player decrypts, the proxy only
  carries the key request with the right headers).
- **Test:** `cargo test -p stream_proxy` (local fake origin servers, race tests for open/close/shutdown).
