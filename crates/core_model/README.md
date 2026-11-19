# core_model

Pure data types shared across the app. No I/O.

- **Public API:** see `src/lib.rs`.
- **`redact_url`:** `host#fingerprint` of a stream address for logs (stream links carry tokens); use it wherever an address is logged.
- **`DEFAULT_STREAM_USER_AGENT`:** Chrome desktop string matching CloudStream Android (`MainAPI.USER_AGENT`) when a link names no User-Agent; shared by stream check, mpv, and the stream proxy so a host that accepts the Android player is not failed by a probe.
- **Test:** `cargo test -p core_model`
- **Rules:** follow the workspace `RULES.md`.
