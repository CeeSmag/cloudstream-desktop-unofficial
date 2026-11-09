# Releasing CloudStream RS

A release is three files, built from one tagged commit:

| File | Built by | Where |
|---|---|---|
| `CloudStream_<ver>_x64-setup.exe` | `scripts/build_release.ps1` | Windows x64 installer |
| `CloudStream_<ver>_x64_portable.exe` | `scripts/build_release.ps1` | Windows x64, one file, no install |
| `CloudStream-<ver>-x86_64.AppImage` / `-aarch64.AppImage` | `scripts/build_release.sh` | Linux, one per CPU type |

The in-app updater reads the GitHub release and picks the file by its name (`*-setup.exe`,
`*_portable.exe`, and the `*.AppImage` whose name contains `x86_64` or `aarch64`), so keep these names.

## The normal way: let GitHub build it

1. Bump the version to the new number in four places: `Cargo.toml` (`[workspace.package]`), `Cargo.lock`
   (the workspace crates; `cargo check -p desktop_tauri` refreshes it), `crates/desktop_tauri/tauri.conf.json`
   and `crates/desktop_tauri/frontend/package.json`.
2. Run the gates (see `RULES.md` §8): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, and in `crates/desktop_tauri/frontend` `npm test` and `npm run build`.
3. Commit, push to `main`, then push the tag:
   ```bash
   git tag v0.3.4 && git push origin v0.3.4
   ```
4. The **Release build** workflow (`.github/workflows/release.yml`) builds everything and creates a **draft**
   release with all the files: Windows x64 on `windows-latest`, the AppImages on native x86_64 and ARM
   runners inside Debian 12 (so they run on most distros).
5. Open the draft, check the files, edit the notes and press **Publish**.

The tag must match the version in `tauri.conf.json`: the workflow does not check it, and the files are
named after the version in `tauri.conf.json`.

To try the build without releasing, run the workflow by hand (Actions > Release build > Run workflow). The files
are then downloadable artifacts of the run and no release is made.

## Building by hand

Each script builds on its own OS and writes to `dist/`.

**Windows** needs the Rust MSVC toolchain, Node 20+, JDK 21 and the sidecar built once:
```powershell
powershell -File scripts\setup_sidecar.ps1
powershell -ExecutionPolicy Bypass -File scripts\build_release.ps1            # installer + portable
powershell -ExecutionPolicy Bypass -File scripts\build_release.ps1 -Kind portable
```
Close a running portable exe first: Windows will not overwrite a running file.

**Linux** (Debian or Ubuntu; run once with `--install-deps`):
```bash
scripts/build_release.sh --install-deps
scripts/build_release.sh        # AppImage for this PC's CPU
```

## What every package carries

`libmpv-2.dll`, `vulkan-1.dll`, the extension sidecar and a Java 21 runtime, so the app runs on a clean PC.

- **`vulkan-1.dll`** comes from `scripts/vendor/vulkan/x64/` (built from Khronos' open-source loader; see the README
  there). libmpv imports it and PCs without a GPU driver have none, so without it the player fails to load.
  The Windows script stops if it is missing or is not an x64 file, and it checks that the file is staged for the
  installer and present in the portable payload.
- **The portable exe** is a small launcher (`crates/portable_launcher`) with the whole app packed inside; on first
  run it unpacks to `%LOCALAPPDATA%\CloudStreamRs\runtime-<version>` and starts from there. It carries the
  CloudStream icon from `icon.ico`.
- **The sidecar** is built from a pinned commit of the reference desktop client plus our `sidecar/` overlay.

## Before you publish

Run the portable exe on a PC (or VM) with no GPU driver and no Vulkan SDK and play a video. If libmpv fails to
load, read `<data>\logs\cloudstream.log`. Then, in the app, Settings > General > Check for update should report the
new version as the latest once the release is published.
