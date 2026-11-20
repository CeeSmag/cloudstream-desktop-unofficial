#!/usr/bin/env bash
# Collect whole-workspace coverage with cargo-llvm-cov in a single test pass,
# emit LCOV + HTML + JSON under target/coverage/, and check per-crate thresholds.
#
# Usage:
#   scripts/coverage.sh                      # enforce thresholds
#   scripts/coverage.sh --report-only        # print report without failing on thresholds
#   scripts/coverage.sh --show-gaps 30       # list top 30 files by uncovered lines
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

OUT_DIR="${REPO_ROOT}/target/coverage"
mkdir -p "${OUT_DIR}"

echo "==> Running instrumented workspace tests (cargo llvm-cov --workspace --no-report)"
cargo llvm-cov --workspace --no-report

echo "==> Writing LCOV report (${OUT_DIR}/lcov.info)"
cargo llvm-cov report --lcov --output-path "${OUT_DIR}/lcov.info"

echo "==> Writing HTML report (${OUT_DIR}/html/index.html)"
cargo llvm-cov report --html --output-dir "${OUT_DIR}/html"

echo "==> Writing JSON summary (${OUT_DIR}/summary.json)"
cargo llvm-cov report --json --output-path "${OUT_DIR}/summary.json"

echo "==> Evaluating per-crate coverage thresholds"
python3 "${SCRIPT_DIR}/check_coverage.py" --json "${OUT_DIR}/summary.json" "$@"
