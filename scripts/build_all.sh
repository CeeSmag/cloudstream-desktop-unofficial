#!/usr/bin/env bash
# Builds the Windows single-file portable exe and the Linux AppImage into dist/.
# Usage: scripts/build_all.sh [all|windows|linux]   (default: all)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="${1:-all}"

case "$TARGET" in
    all|windows|linux) ;;
    *) echo "usage: $0 [all|windows|linux]" >&2; exit 2 ;;
esac

if [[ "$TARGET" == "all" || "$TARGET" == "windows" ]]; then
    echo ">>> Windows single-exe portable"
    bash "$SCRIPT_DIR/build_windows_portable.sh"
fi
if [[ "$TARGET" == "all" || "$TARGET" == "linux" ]]; then
    echo ">>> Linux AppImage"
    bash "$SCRIPT_DIR/build_appimage.sh"
fi

echo
echo "Artifacts in $REPO_ROOT/dist:"
ls -lh "$REPO_ROOT/dist"
