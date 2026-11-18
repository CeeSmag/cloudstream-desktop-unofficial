# extension_runtime

Loads and executes extensions behind the ExtensionRuntime trait.

- **Windows:** the sidecar is constrained with a Job Object (memory cap + kill-on-close); start fails if the job cannot be applied.
- **Public API:** see `src/lib.rs`.
- **Test:** `cargo test -p extension_runtime`
- **Rules:** follow the workspace `RULES.md`.
