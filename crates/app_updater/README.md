# app_updater

Self-updater: finds the latest GitHub release, checks its signed checksum list, downloads the
file for this system, checks its SHA-256, then starts an install helper. Nothing unchecked is
installed. Ported from the former Tauri shell.

## Public API (`lib.rs`)

1. `check_latest` → `UpdateCheck`: reads the latest release and picks the asset for this system by
   name (`*setup.exe` for an installed Windows app, `*portable.exe` when portable / `portable.txt` /
   `CS_PORTABLE_EXE` / `APPIMAGE`, else `*.AppImage` matching `x86_64`/`aarch64`).
2. `download_and_install(data_dir, on_progress)`: downloads `SHA256SUMS` and `SHA256SUMS.minisig`,
   verifies the signature with the built-in key, downloads the asset into a fresh
   `<data>/updates/<attempt>/`, compares its SHA-256 to the signed list, then starts
   `install.bat` / `install.sh` to replace the app after exit. Any failure removes the folder.
   Caller closes the app on `Ok`. Progress via `DownloadProgress`.
3. `remove_stale_downloads(data_dir)`: clears folders of earlier attempts.

Errors: `UpdateError`. A release without `SHA256SUMS` + `SHA256SUMS.minisig`, or a build whose
`update_signing_key.pub` does not decode as a minisign public key, installs nothing.

## Signing key

`update_signing_key.pub` is the owner's minisign public key (key ID `D09F1A068C469C6B`). It is
embedded at compile time (`include_str!` → `UPDATE_SIGNING_KEY` in `update_verify.rs`). Only
`SHA256SUMS` signed with the matching secret key is accepted; legacy (non-prehashed) minisign
signatures are refused.

Release files must keep their names and be signed with `scripts/sign_release.sh` (see
`docs/RELEASING.md`).

Test: `cargo test -p app_updater` (version rules, asset choice, signature and hash checks with a
key made per test, helper script text, folders; plus a decode check of `update_signing_key.pub`).
