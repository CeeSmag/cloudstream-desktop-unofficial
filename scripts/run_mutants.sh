#!/usr/bin/env bash
# Run mutation testing with `cargo-mutants` (Rust) and PITest (JVM sidecar).
#
# Usage:
#   scripts/run_mutants.sh --core                    # run all 6 Rust core logic crates
#   scripts/run_mutants.sh --crate core_model        # run a single Rust crate
#   scripts/run_mutants.sh --diff [base_ref]         # mutate only Rust lines changed vs base_ref (default: HEAD~1)
#   scripts/run_mutants.sh --in-diff path/to/diff    # mutate Rust lines in an existing unified diff
#   scripts/run_mutants.sh --sidecar [targets]       # run PITest across plugin-runtime, android-stubs, cs-sidecar, api-scanner
#   scripts/run_mutants.sh --all                     # run both --core (Rust) and --sidecar (JVM)
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

SIDECAR_MODULES=(
    plugin-runtime
    cs-sidecar
    api-scanner
)

OUT_ROOT="${REPO_ROOT}/target/mutants"
SIDECAR_REPO="${REPO_ROOT}/target/bundle-cache/sidecar-repo"
mkdir -p "${OUT_ROOT}"

usage() {
    sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

MODE="core"
SINGLE_CRATE=""
DIFF_REF="HEAD~1"
DIFF_FILE=""
SIDECAR_TARGETS=""
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
        --sidecar)
            MODE="sidecar"
            if [[ $# -ge 2 && "$2" != --* ]]; then
                SIDECAR_TARGETS="$2"
                shift 2
            else
                shift
            fi
            ;;
        --all)
            MODE="all"
            shift
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

require_cargo_mutants() {
    command -v cargo-mutants >/dev/null 2>&1 || {
        echo "ERROR: cargo-mutants is not installed (cargo binstall -y cargo-mutants)" >&2
        exit 1
    }
}

find_jdk21() {
    if [[ -n "${CS_SIDECAR_JAVA_HOME:-}" && -x "${CS_SIDECAR_JAVA_HOME}/bin/javac" ]]; then
        echo "${CS_SIDECAR_JAVA_HOME}"
        return
    fi
    if [[ -n "${JAVA_HOME:-}" && -x "${JAVA_HOME}/bin/javac" ]]; then
        echo "${JAVA_HOME}"
        return
    fi
    for d in /usr/lib/jvm/java-21-openjdk /usr/lib/jvm/java-21-openjdk-amd64 /usr/lib/jvm/java-21-openjdk-arm64 /usr/lib/jvm/temurin-21; do
        if [[ -x "${d}/bin/javac" ]]; then
            echo "${d}"
            return
        fi
    done
    echo "ERROR: JDK 21 not found (set JAVA_HOME or CS_SIDECAR_JAVA_HOME)" >&2
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

summarize_pitest() {
    local label="$1"
    local xml_path="$2"
    python3 - "${label}" "${xml_path}" <<'EOF'
import sys, xml.etree.ElementTree as ET
from pathlib import Path

label, xml_file = sys.argv[1], Path(sys.argv[2])
if not xml_file.is_file():
    print(f"  {label:<18} (no mutations.xml found)")
    sys.exit(0)

root = ET.parse(xml_file).getroot()
killed = survived = no_cov = timed_out = other = 0
for m in root.findall("mutation"):
    st = m.attrib.get("status", "")
    if st == "KILLED":
        killed += 1
    elif st == "SURVIVED":
        survived += 1
    elif st == "NO_COVERAGE":
        no_cov += 1
    elif st == "TIMED_OUT":
        timed_out += 1
    else:
        other += 1

total = killed + survived + no_cov + timed_out + other
covered = killed + survived + timed_out
score_all = (100.0 * killed / total) if total else 100.0
score_cov = (100.0 * killed / covered) if covered else 100.0
print(
    f"  {label:<18} killed={killed:4d}  survived={survived:4d}  no_cov={no_cov:4d}  "
    f"timeout={timed_out:2d}  score={score_all:5.1f}% (covered={score_cov:5.1f}%)"
)
EOF
}

run_rust_core() {
    require_cargo_mutants
    echo "==> Running cargo-mutants across ${#CORE_CRATES[@]} core logic crates"
    for crate in "${CORE_CRATES[@]}"; do
        echo "--- Mutating ${crate} ---"
        cargo mutants -p "${crate}" --timeout 120 --output "${OUT_ROOT}/${crate}" --no-shuffle "${EXTRA_ARGS[@]}" || true
    done
    echo
    echo "==> Core Mutation Testing Summary (Rust)"
    for crate in "${CORE_CRATES[@]}"; do
        summarize_dir "${crate}" "${OUT_ROOT}/${crate}"
    done
}

run_sidecar_pitest() {
    local jdk
    jdk="$(find_jdk21)"
    echo "==> Preparing sidecar repo for PITest (JDK: ${jdk})"
    "${SCRIPT_DIR}/prepare_sidecar_repo.sh" "${SIDECAR_REPO}"
    local gradle_args=(--no-daemon --continue "-Dorg.gradle.jvmargs=-Xmx4G -Dfile.encoding=UTF-8")
    if [[ -n "${SIDECAR_TARGETS}" ]]; then
        gradle_args+=("-PpitestTargets=${SIDECAR_TARGETS}")
    fi
    local tasks=()
    for mod in "${SIDECAR_MODULES[@]}"; do
        tasks+=(":${mod}:pitest")
    done
    echo "==> Running PITest (${tasks[*]})"
    JAVA_HOME="${jdk}" "${SIDECAR_REPO}/gradlew" -p "${SIDECAR_REPO}" "${gradle_args[@]}" "${tasks[@]}" || true
    echo
    echo "==> Sidecar Mutation Testing Summary (PITest)"
    for mod in "${SIDECAR_MODULES[@]}"; do
        local src_report="${SIDECAR_REPO}/${mod}/build/reports/pitest"
        local dst_report="${OUT_ROOT}/sidecar/${mod}"
        if [[ -d "${src_report}" ]]; then
            rm -rf "${dst_report}"
            mkdir -p "${dst_report}"
            cp -rf "${src_report}/." "${dst_report}/"
        fi
        summarize_pitest "${mod}" "${dst_report}/mutations.xml"
    done
    echo "HTML reports: ${OUT_ROOT}/sidecar/<module>/index.html"
}

case "${MODE}" in
    diff)
        require_cargo_mutants
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
        require_cargo_mutants
        echo "==> Running cargo-mutants on ${DIFF_FILE}"
        cargo mutants --in-diff "${DIFF_FILE}" --timeout 120 --output "${OUT_ROOT}/diff" --no-shuffle "${EXTRA_ARGS[@]}" || true
        summarize_dir "diff" "${OUT_ROOT}/diff"
        ;;
    crate)
        require_cargo_mutants
        echo "==> Running cargo-mutants on crate ${SINGLE_CRATE}"
        cargo mutants -p "${SINGLE_CRATE}" --timeout 120 --output "${OUT_ROOT}/${SINGLE_CRATE}" --no-shuffle "${EXTRA_ARGS[@]}" || true
        summarize_dir "${SINGLE_CRATE}" "${OUT_ROOT}/${SINGLE_CRATE}"
        ;;
    core)
        run_rust_core
        ;;
    sidecar)
        run_sidecar_pitest
        ;;
    all)
        run_rust_core
        echo
        run_sidecar_pitest
        ;;
esac
