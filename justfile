# Developer tasks. Needs: just, cargo-nextest (optional), cargo-deny, cargo-audit.
# Live tests need the JVM sidecar, libmpv and network: see README of each crate.

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# CI (.github/workflows/ci.yml) runs the same checks, on Windows and Linux: change the two together.
# Everything that must pass before a commit (RULES.md section 8).
gates: fmt clippy test deny audit lengths

fmt:
    cargo fmt --all -- --check

[windows]
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Without cognitive_complexity, as in CI's Linux job: `tracing/log` (on through calloop) makes clippy
# count each tracing macro's `log` branch as complexity; the Windows check covers the lint.
[unix]
clippy:
    cargo clippy --workspace --all-targets -- -D warnings -A clippy::cognitive_complexity

# Unit and integration tests; uses nextest when installed.
[windows]
test:
    cargo nextest run --workspace; if ($LASTEXITCODE -ne 0) { cargo test --workspace }

[unix]
test:
    if command -v cargo-nextest >/dev/null 2>&1; then cargo nextest run --workspace; else cargo test --workspace; fi

deny:
    cargo deny check

audit:
    cargo audit

# Rust files outside the tests within 300 lines (RULES.md section 4; clippy only counts functions).
[windows]
lengths:
    python -m unittest discover -s scripts -p "*_test.py"
    python scripts/check_file_lengths.py

[unix]
lengths:
    python3 -m unittest discover -s scripts -p "*_test.py"
    python3 scripts/check_file_lengths.py

# Ignored tests that need the sidecar, libmpv and network (set the CS_* variables first).
live:
    cargo test --workspace -- --ignored --test-threads=1

# Rebuild and start the app.
[windows]
run:
    cargo run -p app_shell

# On Linux, reuse the sidecar/JRE from `scripts/build_release.sh` when present.
[unix]
run:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(cd "{{justfile_directory}}" && pwd)"
    cache="$root/target/bundle-cache"
    if [[ -z "${CS_SIDECAR_HOME:-}" && -d "$cache/cs-sidecar-dist/bin" ]]; then
      export CS_SIDECAR_HOME="$cache/cs-sidecar-dist"
    fi
    if [[ -z "${CS_SIDECAR_JAVA_HOME:-}" ]]; then
      if [[ -d "$cache/jre-linux-x64/bin" ]]; then
        export CS_SIDECAR_JAVA_HOME="$cache/jre-linux-x64"
      elif [[ -d /usr/lib/jvm/java-21-openjdk ]]; then
        export CS_SIDECAR_JAVA_HOME=/usr/lib/jvm/java-21-openjdk
      fi
    fi
    cargo run -p app_shell

# Build the JVM sidecar the app needs for extensions.
[windows]
sidecar:
    powershell -File scripts/setup_sidecar.ps1

[unix]
sidecar:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "On Linux, build the AppImage once (scripts/build_release.sh) or set CS_SIDECAR_HOME to target/bundle-cache/cs-sidecar-dist"
    exit 1
