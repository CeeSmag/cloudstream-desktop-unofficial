# extension_runtime

Loads and executes extensions behind the ExtensionRuntime trait.

- **Windows:** the sidecar JVM starts inside an AppContainer (fail-closed) and is also constrained with a Job Object (memory cap + kill-on-close).
- **Public API:** see `src/lib.rs`.
- **Test:** `cargo test -p extension_runtime`
- **Rules:** follow the workspace `RULES.md`.
