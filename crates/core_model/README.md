# core_model

Pure data types shared across the app. No I/O.

- **Public API:** see `src/lib.rs`.
- **`redact_url`:** `host#fingerprint` of a stream address for logs (stream links carry tokens); use it wherever an address is logged.
- **Test:** `cargo test -p core_model`
- **Rules:** follow the workspace `RULES.md`.
