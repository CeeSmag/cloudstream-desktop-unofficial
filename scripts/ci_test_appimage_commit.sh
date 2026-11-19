#!/usr/bin/env bash
# Run the local GitHub CI gates, the full workspace test suite, build the Linux
# AppImage, then create a git commit — stopping at the first failure with a clear
# report of which stage and command failed.
#
# Mirrors `.github/workflows/ci.yml` / `just gates` for what Linux can run:
#   fmt, clippy, deny, audit, file lengths, cargo test --workspace
# Sidecar Kotlin tests stay Windows-only in CI; they are skipped here (noted).
#
# Usage:
#   scripts/ci_test_appimage_commit.sh -m "Commit message"
#   scripts/ci_test_appimage_commit.sh "Commit message"
#   scripts/ci_test_appimage_commit.sh --help
#
# Env:
#   CS_OUT_DIR — passed through to scripts/build_release.sh (AppImage output dir)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# Avoid Cursor/rustup proxy wrappers that break cargo in some agent shells.
unset RUSTC_WRAPPER RUSTUP_TOOLCHAIN 2>/dev/null || true
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

COMMIT_MSG=""
STAGE_NUM=0
STAGE_NAME=""
FAILED_CMD=""

usage() {
    # Header comments only (lines starting with `# ` after the shebang).
    sed -n '2,/^set -euo pipefail$/p' "${BASH_SOURCE[0]}" | sed '$d; s/^# \{0,1\}//'
}

die() {
    echo "ERROR: $*" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || die "'$1' is missing (needed for this script)."
}

# Print a boxed failure for the stage that just died; then exit with its code.
on_err() {
    local code=$?
    echo >&2
    echo "════════════════════════════════════════════════════════════" >&2
    echo "FAILED — stage ${STAGE_NUM}: ${STAGE_NAME}" >&2
    if [[ -n "${FAILED_CMD}" ]]; then
        echo "Command: ${FAILED_CMD}" >&2
    fi
    echo "Exit code: ${code}" >&2
    echo "════════════════════════════════════════════════════════════" >&2
    exit "${code}"
}
trap on_err ERR

run() {
    FAILED_CMD="$*"
    echo "    \$ $*"
    "$@"
    FAILED_CMD=""
}

begin_stage() {
    STAGE_NUM=$1
    STAGE_NAME=$2
    echo
    echo "==> Stage ${STAGE_NUM}/4: ${STAGE_NAME}"
}

# --- args -----------------------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        -m|--message)
            [[ $# -ge 2 ]] || die "-m / --message needs a commit message"
            COMMIT_MSG=$2
            shift 2
            ;;
        --)
            shift
            break
            ;;
        -*)
            die "unknown argument: $1 (try --help)"
            ;;
        *)
            if [[ -z "${COMMIT_MSG}" ]]; then
                COMMIT_MSG=$1
                shift
            else
                die "unexpected extra argument: $1"
            fi
            ;;
    esac
done

[[ -n "${COMMIT_MSG}" ]] || die "pass a commit message: scripts/ci_test_appimage_commit.sh -m \"…\""

need cargo
need python3
need git
need rustfmt
# clippy is invoked as `cargo clippy`
cargo clippy -V >/dev/null 2>&1 || die "clippy is missing (rustup component add clippy)"
command -v cargo-deny >/dev/null 2>&1 || die "cargo-deny is missing (cargo install cargo-deny)"
command -v cargo-audit >/dev/null 2>&1 || die "cargo-audit is missing (cargo install cargo-audit)"

STARTED=$(date +%s)
echo "ci_test_appimage_commit: repo=${REPO_ROOT}"
echo "Commit message: ${COMMIT_MSG}"

# --- 1. Local GitHub CI gates (non-test jobs from ci.yml / just gates) ---------------------------
begin_stage 1 "Local GitHub CI gates (fmt, clippy, deny, audit, file lengths)"

echo "  [1a] cargo fmt --all -- --check"
run cargo fmt --all -- --check

echo "  [1b] cargo clippy --workspace --all-targets (Linux CI flags)"
# Same allowance as CI's rust-linux job / justfile [unix] clippy.
run cargo clippy --workspace --all-targets -- -D warnings -A clippy::cognitive_complexity

echo "  [1c] cargo deny check"
run cargo deny check

echo "  [1d] cargo audit"
run cargo audit

echo "  [1e] file-length unit tests + check (project-rules)"
run python3 -m unittest discover -s scripts -p "*_test.py"
run python3 scripts/check_file_lengths.py

echo "  note: CI sidecar Kotlin job is Windows-only; skipped on this Linux run."
echo "  Stage 1 OK"

# --- 2. All workspace tests ---------------------------------------------------------------------
begin_stage 2 "All workspace tests (cargo test --workspace)"

if command -v cargo-nextest >/dev/null 2>&1; then
    echo "  using cargo-nextest (same as just test on Unix when installed)"
    run cargo nextest run --workspace
else
    run cargo test --workspace
fi
echo "  Stage 2 OK"

# --- 3. AppImage --------------------------------------------------------------------------------
begin_stage 3 "Build Linux AppImage (scripts/build_release.sh)"

run bash "${SCRIPT_DIR}/build_release.sh"
echo "  Stage 3 OK"

# --- 4. Commit ----------------------------------------------------------------------------------
begin_stage 4 "Git commit"

# Do not touch git config. Stage everything that belongs in the working tree except
# typical secrets; refuse if nothing changed.
if git status --porcelain | grep -qE '(^.. \.env)|(^.. .*\.pem$)|(credentials\.json)'; then
    die "refusing to commit: working tree looks like it contains secrets (.env / .pem / credentials.json). Unstage or remove them first."
fi

STATUS_BEFORE=$(git status --porcelain)
if [[ -z "${STATUS_BEFORE}" ]]; then
    die "nothing to commit (working tree clean after a successful build). Make your changes first, or skip stage 4."
fi

echo "  staging all changes (git add -A)"
run git add -A

# Re-check after staging in case add was a no-op for ignored files only.
if git diff --cached --quiet; then
    die "nothing staged to commit (only ignored/untracked-empty?). Working tree after build may be clean of tracked changes."
fi

echo "  creating commit"
FAILED_CMD="git commit -m <message>"
git commit -m "$(cat <<EOF
${COMMIT_MSG}

EOF
)"
FAILED_CMD=""

echo "  Stage 4 OK"
git status --short --branch

ELAPSED=$(( $(date +%s) - STARTED ))
echo
echo "==> All 4 stages passed in ${ELAPSED}s"
echo "    1 CI gates · 2 tests · 3 AppImage · 4 commit"
echo "    Tip: push only when you intend to (this script does not push)."
