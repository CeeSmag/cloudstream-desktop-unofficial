#!/usr/bin/env bash
# Prepares the Gradle checkout the JVM sidecar is built in: the pinned CS3 desktop client, our
# overlay, the checked ExtensionLoader patches and our modules (cs-sidecar, api-scanner).
# Used by scripts/build_release.sh and tools/extension_recorder; Windows: scripts/setup_sidecar.ps1.
#
# Usage: scripts/prepare_sidecar_repo.sh <repo-dir>
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
repo="${1:?usage: prepare_sidecar_repo.sh <repo-dir>}"
commit="2d5a86ce2e7466494a9084776dcddd3626f633dd"
url="https://github.com/errorcode26/CS3-desktop-client-unofficial.git"
# Our Gradle modules inside that checkout. Only cs-sidecar goes into packages; api-scanner is a
# development tool (it scans plugins for API members the runtime lacks).
modules=(cs-sidecar api-scanner)

if [ ! -d "$repo/.git" ]; then git clone --no-checkout "$url" "$repo"; fi
(
    cd "$repo"
    # Not into submodules: newer upstream commits point at submodule commits that are gone;
    # the pinned one is checked out by the submodule update below.
    git fetch --quiet --no-recurse-submodules origin "$commit"
    git checkout --quiet "$commit"
    git submodule update --init --depth 1
)
cp -rf "$REPO_ROOT/sidecar/overlay/"* "$repo/"
python3 "$REPO_ROOT/scripts/patch_extension_loader.py" "$repo"
for module in "${modules[@]}"; do
    rm -rf "${repo:?}/$module"
    cp -rf "$REPO_ROOT/sidecar/$module" "$repo/$module"
    grep -q "include(\":$module\")" "$repo/settings.gradle.kts" || printf '\ninclude(":%s")\n' "$module" >> "$repo/settings.gradle.kts"
done
