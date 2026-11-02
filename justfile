# Developer tasks. Needs: just, cargo-nextest (optional), cargo-deny, cargo-audit.
# Live tests need the JVM sidecar, libmpv and network: see README of each crate.

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# Everything that must pass before a commit (RULES.md section 8).
gates: fmt clippy test deny audit

fmt:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Unit and integration tests; uses nextest when installed.
test:
    cargo nextest run --workspace; if ($LASTEXITCODE -ne 0) { cargo test --workspace }

deny:
    cargo deny check

audit:
    cargo audit

# Ignored tests that need the sidecar, libmpv and network (set the CS_* variables first).
live:
    cargo test --workspace -- --ignored --test-threads=1

# Rebuild and start the app.
run:
    cargo run -p app_shell

# Build the JVM sidecar the app needs for extensions.
sidecar:
    powershell -File scripts/setup_sidecar.ps1
