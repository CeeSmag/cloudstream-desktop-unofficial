# portable_launcher

The single-file Windows portable build (`CloudStream_<ver>_x64_portable.exe`).

The release script packs the whole app (`desktop_tauri.exe`, `libmpv-2.dll`, `vulkan-1.dll`, the sidecar and a Java
runtime) into a tar.gz and hands it to this crate through the `CS_PAYLOAD_ARCHIVE` environment variable; the
payload is compiled into the exe. On start the launcher unpacks it once per version to
`%LOCALAPPDATA%\CloudStreamRs\runtime-<version>\` (Linux: `$XDG_DATA_HOME` or `~/.local/share`), starts
`desktop_tauri` from there and passes along its own path as `CS_PORTABLE_EXE` (the updater uses it to replace
the portable file). User data stays in the normal `CloudStreamRs` data folder, so updating the exe keeps it.

`build.rs` also embeds `crates/desktop_tauri/icons/icon.ico` as the exe's icon (Windows only).

Without `CS_PAYLOAD_ARCHIVE` the crate still builds (for `cargo check` and clippy), but the exe has no payload
and reports that when started.

## Build

```powershell
powershell -File scripts\build_release.ps1 -Kind portable
```
