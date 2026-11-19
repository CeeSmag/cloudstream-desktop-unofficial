# Releasing CloudStream RS (egui)

A release is three files, built from one tagged commit:

| File | Built by | Where |
|---|---|---|
| `CloudStream_<ver>_x64-setup.exe` | `scripts/build_release.ps1` | Windows x64 installer (NSIS) |
| `CloudStream_<ver>_x64_portable.exe` | `scripts/build_release.ps1` | Windows x64, one file, no install |
| `CloudStream-<ver>-x86_64.AppImage` / `-aarch64.AppImage` | `scripts/build_release.sh` | Linux, one per CPU type |

This fork ships the egui shell (`cloudstream` from `app_shell`), not the Tauri/WebView UI.
The in-app updater (when ported) still picks files by name (`*-setup.exe`, `*_portable.exe`, and the
`*.AppImage` whose name contains `x86_64` or `aarch64`), so keep these names.

## Signed releases (the app refuses unsigned ones)

From the version after 0.4.2 the app installs an update only when the release carries `SHA256SUMS`
(`sha256sum` of every release file) and `SHA256SUMS.minisig` (its minisign signature), the signature
is valid for the public key in `crates/app_updater/update_signing_key.pub`, and the downloaded file's
SHA-256 is the one in the list. Otherwise it shows why and installs nothing.

The key is the owner's minisign key `D09F1A068C469C6B`. Its secret half is kept offline by the owner
and in the GitHub secrets `MINISIGN_SECRET_KEY` and `MINISIGN_PASSWORD`; it is never in the repository.
Changing the key in the file means every copy already installed rejects releases signed with the new
key: users would have to download one version by hand.

Signing is done by `scripts/sign_release.sh`: it writes `SHA256SUMS` for the `*-setup.exe`,
`*_portable.exe` and `*.AppImage` files in a folder, signs it, and checks the signature against the
app's key, so a wrong key or password fails there instead of on users' PCs. The release workflow runs
it on every tagged build and makes no draft when a secret is missing.

Before pressing **Publish**, check the draft lists `SHA256SUMS` and `SHA256SUMS.minisig` next to the
three files. To check by hand: download all of them into one folder and run
`minisign -Vm SHA256SUMS -p crates/app_updater/update_signing_key.pub` and `sha256sum -c SHA256SUMS`.

## The normal way: let GitHub build it

1. Bump the version in `Cargo.toml` (`[workspace.package]`) and refresh `Cargo.lock`
   (`cargo check -p app_shell`).
2. Run the gates (see `RULES.md` §8): `just gates` (or the equivalent cargo/python commands).
3. Commit, push to `main`, then push the tag:
   ```bash
   git tag v0.3.4 && git push origin v0.3.4
   ```
4. The **Release build** workflow (`.github/workflows/release.yml`) builds everything and creates a **draft**
   release with all the files: Windows x64 on `windows-latest`, the AppImages on native x86_64 and ARM
   runners inside Debian 12 (so they run on most distros).
5. Open the draft, check the files (including `SHA256SUMS` and `SHA256SUMS.minisig`), edit the notes and
   press **Publish**.

The tag should match the workspace version in `Cargo.toml`: the workflow does not check it, and the files are
named after that version.

To try the build without releasing, run the workflow by hand (Actions > Release build > Run workflow). The files
are then downloadable artifacts of the run, unsigned, and no release is made.

## Building by hand

Each script builds on its own OS and writes to `dist/`.

**Windows** needs the Rust MSVC toolchain, NSIS (`makensis`; `choco install nsis`), JDK 21 and the sidecar built once:
```powershell
powershell -File scripts\setup_sidecar.ps1
powershell -ExecutionPolicy Bypass -File scripts\build_release.ps1            # installer + portable
powershell -ExecutionPolicy Bypass -File scripts\build_release.ps1 -Kind portable
```
Close a running portable exe first: Windows will not overwrite a running file.

**Linux** (Debian or Ubuntu; run once with `--install-deps`):
```bash
bash scripts/build_release.sh --install-deps
bash scripts/build_release.sh        # AppImage for this PC's CPU
```

**Signing hand-built files** (Linux, or Git Bash on Windows with `winget install jedisct1.minisign`): put
only this version's files in one folder, then
```bash
MINISIGN_SECRET_KEY_FILE=/path/to/cloudstream.key scripts/sign_release.sh dist
```
minisign asks for the password (or set `MINISIGN_PASSWORD`). Upload `SHA256SUMS` and `SHA256SUMS.minisig`
with the files. Without a key the script writes only `SHA256SUMS`, and the app refuses the release.

## What every package carries

`libmpv` (DLL or `.so`), `vulkan-1.dll` on Windows, the extension sidecar and a Java 21 runtime, so the app runs on a clean PC.

- **`vulkan-1.dll`** comes from `scripts/vendor/vulkan/x64/` (built from Khronos' open-source loader; see the README
  there). libmpv imports it and PCs without a GPU driver have none, so without it the player fails to load.
  The Windows script stops if it is missing or is not an x64 file, and it checks that the file is staged for the
  installer and present in the portable payload.
- **The portable exe** is a small launcher (`crates/portable_launcher`) with the whole app packed inside; on first
  run it unpacks to `%LOCALAPPDATA%\CloudStreamRs\runtime-<version>` and starts `cloudstream.exe` from there. It carries the
  CloudStream icon from `crates/app_shell/icons/icon.ico`.
- **The sidecar** is built from a pinned commit of the reference desktop client plus our `sidecar/` overlay.

## Before you publish

Run the portable exe on a PC (or VM) with no GPU driver and no Vulkan SDK and play a video. If libmpv fails to
load, read `<data>\logs\cloudstream.log`.
