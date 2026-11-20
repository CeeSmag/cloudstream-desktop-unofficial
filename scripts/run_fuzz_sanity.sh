#!/usr/bin/env bash
# Time-boxed fuzz sanity runner.
# Runs deterministic corpus & mutation stress tests on stable Rust, plus optional
# time-boxed `cargo +nightly fuzz run` if cargo-fuzz and nightly are installed.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

SECONDS_PER_TARGET="${CS_FUZZ_SECONDS:-10}"

echo "==> Running stable-Rust fuzz corpus stress tests"
cargo test -p bdd_tests --test fuzz_corpus_stress

if command -v cargo-fuzz >/dev/null 2>&1 && rustup toolchain list | grep -q '^nightly'; then
    echo "==> Running time-boxed libFuzzer targets (${SECONDS_PER_TARGET}s each)"
    for target in subtitle_normalize m3u8_rewrite sidecar_rpc_reply repo_manifest; do
        cargo +nightly fuzz run "${target}" --fuzz-dir fuzz -- -max_total_time="${SECONDS_PER_TARGET}"
    done
else
    echo "note: cargo-fuzz / nightly toolchain not installed; ran stable corpus stress suite only."
fi
