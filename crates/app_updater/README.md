# app_updater

The self-updater, ported from the former Tauri shell. It installs nothing that was not checked.

1. `check_latest`: reads the latest GitHub release and picks the file for this system by name
   (`*-setup.exe` installed Windows app, `*_portable.exe`, `*.AppImage` with `x86_64`/`aarch64`).
2. `download_and_install`: downloads the release's `SHA256SUMS` and `SHA256SUMS.minisig`, checks the
   signature with the key in `update_signing_key.pub`, downloads the file into a fresh
   `<data>/updates/<attempt>/`, compares its SHA-256 with the signed list, then starts the install
   helper (`install.bat` / `install.sh`) that replaces the app after it exits. Any failure removes
   the folder.
3. `remove_stale_downloads`: clears folders of earlier attempts.

Release files must keep their names and be signed with `scripts/sign_release.sh` (see
`docs/RELEASING.md`).

Test: `cargo test -p app_updater` (version rules, asset choice, signature and hash checks with a key
made per test, helper script text, folders).
