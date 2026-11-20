#!/usr/bin/env bash
# Dead-code, unused-dependency, and broken-rustdoc-link gate for all workspace members.
#
# Scans only `[workspace].members` from the root Cargo.toml so non-workspace reference
# directories (`archive/desktop_tauri`, `crates/ui_app`, `spikes/wasmtime_host`, `fuzz`)
# are excluded without duplicating allowlists.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

mapfile -t MEMBERS < <(
    python3 - <<'EOF'
import tomllib
with open("Cargo.toml", "rb") as fh:
    ws = tomllib.load(fh)
for member in ws["workspace"]["members"]:
    print(member)
EOF
)

if [[ ${#MEMBERS[@]} -eq 0 ]]; then
    echo "ERROR: no workspace members found in Cargo.toml" >&2
    exit 1
fi

echo "==> cargo machete (${#MEMBERS[@]} workspace members)"
cargo machete "${MEMBERS[@]}"

echo "==> cargo doc --workspace --no-deps (-D rustdoc::broken_intra_doc_links)"
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace --no-deps
