# extension_repo

Repository manifests and plugin install/update/remove.

- **Install gate:** `ExtensionInstaller::install` requires an HTTPS download URL on the repository host (or GitHub family / subdomain), ZIP magic (`PK`), and a matching `file_size` when the listing publishes one.
- **Public API:** see `src/lib.rs`.
- **Test:** `cargo test -p extension_repo`
- **Rules:** follow the workspace `RULES.md`.
