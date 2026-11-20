#!/usr/bin/env bash
# Run mutation testing with `cargo-mutants` in PR-diff mode or crate-by-crate core mode.
#
# Usage:
#   scripts/run_mutants.sh --core                    # run all 6 core logic crates
#   scripts/run_mutants.sh --crate core_model        # run a single crate
#   scripts/run_mutants.sh --diff [base_ref]         # mutate only lines changed vs base_ref (default: HEAD~1)
#   scripts/run_mutants.sh --in-diff path/to/diff    # mutate lines in an existing unified diff
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

CORE_CRATES=(
    core_model
    skip_times
    title_cache
    subtitle_store
    stream_proxy
    playback_session
)

OUT_ROOT="${REPO_ROOT}/target/mutants"
mkdir -p "${OUT_ROOT}"

usage() {
    sed -n '2,8p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

MODE="core"
SINGLE_CRATE=""
DIFF_REF="HEAD~1"
DIFF_FILE=""
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        --core)
            MODE="core"
            shift
            ;;
        --crate)
            MODE="crate"
            SINGLE_CRATE="${2:?--crate requires a crate name}"
            shift 2
            ;;
        --diff)
            MODE="diff"
            if [[ $# -ge 2 && "$2" != --* ]]; then
                DIFF_REF="$2"
                shift 2
            else
                shift
            fi
            ;;
        --in-diff)
            MODE="in-diff"
            DIFF_FILE="${2:?--in-diff requires a file path}"
            shift 2
            ;;
        --)
            shift
            EXTRA_ARGS+=("$@")
            break
            ;;
        *)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

command -v cargo-mutants >/dev/null 2>&1 || {
    echo "ERROR: cargo-mutants is not installed (cargo binstall -y cargo-mutants)" >&2
    exit 1
}

summarize_dir() {
    local label="$1"
    local dir="$2"
    python3 - "${label}" "${dir}" <<'EOF'
import json, sys
from pathlib import Path

label, out_dir = sys.argv[1], Path(sys.argv[2])
outcomes_path = out_dir / "mutants.out" / "outcomes.json"
if not outcomes_path.exists():
    outcomes_path = out_dir / "outcomes.json"
if not outcomes_path.exists():
    print(f"  {label:<18} (no outcomes.json found)")
    sys.exit(0)

data = json.loads(outcomes_path.read_text(encoding="utf-8"))
caught = missed = timeout = unviable = 0
for item in data.get("outcomes", []):
    if item.get("scenario") == "Baseline":
        continue
    summary = item.get("summary", "")
    if summary == "CaughtMutant":
        caught += 1
    elif summary == "MissedMutant":
        missed += 1
    elif summary == "Timeout":
        timeout += 1
    elif summary == "Unviable":
        unviable += 1

tested = caught + missed + timeout
score = (100.0 * caught / tested) if tested else 100.0
print(f"  {label:<18} caught={caught:3d}  missed={missed:3d}  timeout={timeout:2d}  unviable={unviable:2d}  score={score:5.1f}%")
EOF
}

case "${MODE}" in
    diff)
        TMP_DIFF="${OUT_ROOT}/pr.diff"
        git diff "${DIFF_REF}" -- crates/ > "${TMP_DIFF}"
        if [[ ! -s "${TMP_DIFF}" ]]; then
            echo "==> No crate changes vs ${DIFF_REF}; skipping diff mutation run."
            exit 0
        fi
        echo "==> Running cargo-mutants on diff vs ${DIFF_REF}"
        cargo mutants --in-diff "${TMP_DIFF}" --timeout 120 --output "${OUT_ROOT}/diff" --no-shuffle "${EXTRA_ARGS[@]}" || true
        summarize_dir "diff(${DIFF_REF})" "${OUT_ROOT}/diff"
        ;;
    in-diff)
        echo "==> Running cargo-mutants on ${DIFF_FILE}"
        cargo mutants --in-diff "${DIFF_FILE}" --timeout 120 --output "${OUT_ROOT}/diff" --no-shuffle "${EXTRA_ARGS[@]}" || true
        summarize_dir "diff" "${OUT_ROOT}/diff"
        ;;
    crate)
        echo "==> Running cargo-mutants on crate ${SINGLE_CRATE}"
        cargo mutants -p "${SINGLE_CRATE}" --timeout 120 --output "${OUT_ROOT}/${SINGLE_CRATE}" --no-shuffle "${EXTRA_ARGS[@]}" || true
        summarize_dir "${SINGLE_CRATE}" "${OUT_ROOT}/${SINGLE_CRATE}"
        ;;
    core)
        echo "==> Running cargo-mutants across ${#CORE_CRATES[@]} core logic crates"
        for crate in "${CORE_CRATES[@]}"; do
            echo "--- Mutating ${crate} ---"
            cargo mutants -p "${crate}" --timeout 120 --output "${OUT_ROOT}/${crate}" --no-shuffle "${EXTRA_ARGS[@]}" || true
        done
        echo
        echo "==> Core Mutation Testing Summary"
        for crate in "${CORE_CRATES[@]}"; do
            summarize_dir "${crate}" "${OUT_ROOT}/${crate}"
        done
        ;;
esac
