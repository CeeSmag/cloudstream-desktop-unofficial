# Developer tasks. Needs: just, cargo-nextest (optional), cargo-deny, cargo-audit.
# Live tests need the JVM sidecar, libmpv and network: see README of each crate.

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# CI (.github/workflows/ci.yml) runs the same checks, on Windows and Linux: change the two together.
# Everything that must pass before a commit (RULES.md section 8).
[windows]
gates: fmt clippy dead-code arch unsafe-audit test deny audit lengths

[unix]
gates: fmt clippy clippy-strict dead-code shellcheck arch unsafe-audit test deny audit lengths

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

# Unit, integration, race, property, Slint, and BDD tests; uses nextest when installed.
# Cucumber (`bdd_tests`) uses `harness = false` and does not support nextest's `--list`.
[windows]
test:
    cargo nextest run --workspace --exclude bdd_tests; if ($LASTEXITCODE -eq 0) { cargo test -p bdd_tests } else { cargo test --workspace }

[unix]
test:
    if command -v cargo-nextest >/dev/null 2>&1; then cargo nextest run --workspace --exclude bdd_tests && cargo test -p bdd_tests; else cargo test --workspace; fi

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

# Strict cognitive_complexity clippy pass on Linux (drops Wayland -> tracing/log).
clippy-strict:
    cargo clippy --workspace --all-targets --no-default-features -- -D warnings

# Dead-code, unused dependencies (cargo-machete), and broken rustdoc links.
dead-code:
    bash scripts/check_dead_code.sh

# Lint all shell scripts.
shellcheck:
    shellcheck scripts/*.sh

# Enforce one-way crate architecture and zero cycles (RULES.md §2).
arch:
    python3 scripts/check_architecture.py

# Audit RULES.md §8 unsafe_code allowlist and `// SAFETY:` comments.
unsafe-audit:
    python3 scripts/check_unsafe_audit.py

# Gherkin / Cucumber behavioral test suite.
bdd:
    cargo test -p bdd_tests

# Whole-workspace coverage (HTML + LCOV + JSON under target/coverage/) + thresholds.
coverage:
    bash scripts/coverage.sh --show-gaps 20

# Mutation testing across the 6 core logic crates (or `--diff` for PR diff mode).
mutants:
    bash scripts/run_mutants.sh --core

mutants-diff base="HEAD~1":
    bash scripts/run_mutants.sh --diff "{{base}}"

# Quality metrics summary (Markdown + target/quality/metrics.json).
metrics:
    python3 scripts/quality_metrics.py --markdown-out target/quality/metrics.md

# Time-boxed fuzz corpus stress sanity check.
fuzz-sanity:
    bash scripts/run_fuzz_sanity.sh

# Quick hot-path Criterion benchmark sanity run.
bench-quick:
    cargo bench -p subtitle_store --bench subtitle_normalize -- --quick
    cargo bench -p app_services --bench link_ranking -- --quick
    cargo bench -p stream_proxy --bench hls_rewrite -- --quick

# Full deep QA pass (coverage, core mutants, geiger, metrics, fuzz sanity, quick benches).
qa-full:
    bash scripts/qa_full.sh

# Rebuild and start the app.
[windows]
run:
    cargo run -p app_shell

# On Linux, reuse the sidecar/JRE from `scripts/build_release.sh` when present.
[unix]
run:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(cd "{{justfile_directory()}}" && pwd)"
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
