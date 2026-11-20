#!/usr/bin/env bash
# Deep / nightly QA orchestrator: coverage, mutation testing, unsafe audit (geiger),
# quality metrics summary, fuzz corpus sanity, and quick benchmark sanity.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

MUTANTS_MODE="${1:---core}"

echo "==> [1/6] Whole-workspace coverage + thresholds"
bash "${SCRIPT_DIR}/coverage.sh" --show-gaps 20

echo "==> [2/6] Mutation testing (${MUTANTS_MODE})"
bash "${SCRIPT_DIR}/run_mutants.sh" "${MUTANTS_MODE}"

echo "==> [3/6] Unsafe allowlist audit + cargo-geiger summary"
python3 "${SCRIPT_DIR}/check_unsafe_audit.py" || echo "warn: unsafe allowlist check reported findings"
if command -v cargo-geiger >/dev/null 2>&1; then
    cargo geiger --output-format Ascii --all-features --quiet || true
else
    echo "note: cargo-geiger not installed; skipped transitive unsafe scan."
fi

echo "==> [4/6] Quality metrics report"
python3 "${SCRIPT_DIR}/quality_metrics.py" --markdown-out target/quality/metrics.md

echo "==> [5/6] Fuzz corpus sanity"
bash "${SCRIPT_DIR}/run_fuzz_sanity.sh"

echo "==> [6/6] Hot-path benchmark sanity (--quick)"
cargo bench -p subtitle_store --bench subtitle_normalize -- --quick
cargo bench -p app_services --bench link_ranking -- --quick
cargo bench -p stream_proxy --bench hls_rewrite -- --quick
