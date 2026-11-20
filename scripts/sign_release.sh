#!/usr/bin/env bash
# Writes SHA256SUMS for the release files in a folder and signs it with minisign, so the app's
# updater accepts them (app_updater check + signed SHA256SUMS). Used by the release workflow and
# by hand (Linux, or Git Bash on Windows with minisign installed).
#
#   scripts/sign_release.sh dist                     # sign with the key named below
#   scripts/sign_release.sh --require-signature dir  # fail instead of leaving the files unsigned
#
# The secret key comes from MINISIGN_SECRET_KEY (the key file's text, as in the GitHub secret) or
# MINISIGN_SECRET_KEY_FILE (a path). MINISIGN_PASSWORD is read when set; otherwise minisign asks.
# With no key, only SHA256SUMS is written and the files stay unsigned: the app refuses them.
# After signing, the signature is checked against the public key built into the app, so a wrong
# key or password fails here and not on users' PCs. SIGN_RELEASE_PUBLIC_KEY (a .pub path) replaces
# that key, only to test this script with a throwaway key; the workflow never sets it.
set -euo pipefail

require_signature=false
if [[ "${1:-}" == "--require-signature" ]]; then
    require_signature=true
    shift
fi
folder="${1:?usage: sign_release.sh [--require-signature] <folder with the release files>}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
public_key="${SIGN_RELEASE_PUBLIC_KEY:-$repo_root/crates/app_updater/update_signing_key.pub}"
minisign_bin="${MINISIGN:-minisign}"

if [[ -n "${MINISIGN_SECRET_KEY_FILE:-}" ]]; then
    # Resolve it before changing folder, so a relative path still points at the key.
    MINISIGN_SECRET_KEY_FILE="$(cd "$(dirname "$MINISIGN_SECRET_KEY_FILE")" && pwd)/$(basename "$MINISIGN_SECRET_KEY_FILE")"
fi
cd "$folder"
shopt -s nullglob nocaseglob
release_files=(*-setup.exe *_portable.exe *.AppImage)
shopt -u nocaseglob
if (( ${#release_files[@]} == 0 )); then
    echo "No release files (*-setup.exe, *_portable.exe, *.AppImage) in $folder" >&2
    exit 1
fi

rm -f SHA256SUMS SHA256SUMS.minisig
sha256sum --binary "${release_files[@]}" > SHA256SUMS
echo "SHA256SUMS written for ${#release_files[@]} file(s):"
cat SHA256SUMS

key_file=""
cleanup() { if [[ -n "$key_file" ]]; then rm -f "$key_file"; fi; }
trap cleanup EXIT

if [[ -n "${MINISIGN_SECRET_KEY:-}" ]]; then
    key_file="$(mktemp)"
    chmod 600 "$key_file"
    # The secret may have been saved from Windows with CRLF line ends; minisign wants plain lines.
    printf '%s\n' "$MINISIGN_SECRET_KEY" | tr -d '\r' > "$key_file"
    secret_key="$key_file"
elif [[ -n "${MINISIGN_SECRET_KEY_FILE:-}" ]]; then
    secret_key="$MINISIGN_SECRET_KEY_FILE"
else
    if $require_signature; then
        echo "::error::No signing key (MINISIGN_SECRET_KEY): the release would be refused by the app." >&2
        exit 1
    fi
    echo "WARNING: no signing key given; SHA256SUMS is unsigned and the app will refuse this release." >&2
    exit 0
fi

if [[ -n "${MINISIGN_PASSWORD:-}" ]]; then
    printf '%s\n' "$MINISIGN_PASSWORD" | tr -d '\r' | "$minisign_bin" -S -s "$secret_key" -m SHA256SUMS
else
    "$minisign_bin" -S -s "$secret_key" -m SHA256SUMS
fi

if ! "$minisign_bin" -V -p "$public_key" -m SHA256SUMS; then
    rm -f SHA256SUMS.minisig
    echo "::error::The signature does not match the app's key ($public_key): wrong secret key?" >&2
    exit 1
fi
echo "SHA256SUMS.minisig written and checked against the app's key."
