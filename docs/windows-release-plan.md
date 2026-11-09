# Windows release plan (0.3.3)

Goal: ship the fixes below as a new GitHub release, built on a Windows PC.

Changes since 0.3.2:
- Browse: the Year dropdown now follows the app theme.
- Nav bar: update indicator left of the provider pill; click opens Settings > General.
- Windows: ship `vulkan-1.dll` next to `libmpv-2.dll`. libmpv imports it; PCs with no GPU driver
  have none, so libmpv failed to load and the player did not run.
- Two release scripts: `scripts/build_release.ps1` (Windows: x64 installer + single-file portable exe) and
  `scripts/build_release.sh` (Linux: AppImage for the PC's CPU). No more portable zip.
- `.github/workflows/release.yml` builds all of them on GitHub (Windows x64, AppImage x86_64 and aarch64).

## 0. Prerequisites on the Windows PC
- Rust (MSVC toolchain), Node 20+, `gh` (logged in: `gh auth status`), git.
- Sidecar built: `scripts\setup_sidecar.ps1` (needs JDK 21) -> `C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar`.
- A real x64 `vulkan-1.dll`. `C:\Windows\System32\vulkan-1.dll` works if the Vulkan SDK/runtime is
  installed on that PC (the build script finds it); otherwise pass `-VulkanDll <path>`.
  Do not use the copy from Wine/Proton.

## 1. Get the code
```powershell
git pull origin main
cd crates\desktop_tauri\frontend; npm ci; npm run build; cd ..\..\..
```

## 2. Bump the version to 0.3.3
Same four files as the 0.3.2 bump: `Cargo.toml` (workspace version), `Cargo.lock` (workspace crates),
`crates/desktop_tauri/frontend/package.json`, `crates/desktop_tauri/tauri.conf.json`.
```powershell
cargo check -p desktop_tauri     # refreshes Cargo.lock
```

## 3. Gates (RULES.md)
```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 4. Build
```powershell
.\scripts\build_release.ps1
# -Kind installer|portable builds one; -VulkanDll overrides scripts\vendor\vulkan\x64\vulkan-1.dll
```
Produces in `dist\`: `CloudStream_0.3.3_x64-setup.exe` and `CloudStream_0.3.3_x64_portable.exe`.
The AppImages come from `scripts/build_release.sh` on Linux, or from the GitHub workflow.

## 5. Test before releasing
1. Run the x64 portable exe on this PC: Browse > Year dropdown is dark; the update indicator shows a spinner,
   then a check; clicking it opens Settings > General.
2. Confirm `vulkan-1.dll` is next to `libmpv-2.dll` in the extracted payload.
3. **Main test:** run the portable exe on a PC/VM with no GPU driver and no Vulkan SDK; play a video.
   If libmpv still fails, read `<data>\logs\cloudstream.log`.

## 6. Commit, tag, release
```powershell
git add -A
git commit -m "feat: update indicator, themed year select, ship vulkan loader for GPU-less PCs"
git commit --allow-empty -m "chore(release): bump version to 0.3.3"   # or fold the bump into the commit above
git push origin main
gh release create v0.3.3 dist\CloudStream_0.3.3_x64-setup.exe dist\CloudStream_0.3.3_x64_portable.exe `
  --title "CloudStream RS 0.3.3" --notes "See docs/windows-release-plan.md for changes."
```

## 7. After
- In the app, Settings > General > Check for update should show "latest" on 0.3.3.
- The self-updater reads GitHub releases, so keep asset names as above.
