# desktop_tauri

Tauri v2 application shell providing a modern web UI overlay over hardware-accelerated libmpv video playback.

## Purpose

Hosts the React/Tailwind frontend inside a transparent WebView2 window on Windows, rendering modern frosted-glass controls directly over a native Direct3D/HWND libmpv video surface.

## Public Architecture

- `main.rs`: Entry point initializing `AppPaths`, logging, services, and native `wid` window handle binding.
- `window_mpv.rs`: Safely extracts Win32 `HWND` from the Tauri window.
- `catalog_commands.rs`: IPC commands for provider discovery, home shelves, search, and details.
- `playback_commands.rs`: IPC commands for stream resolution, player state, track selection, and seek.
- `library_commands.rs`: Bookmarks and continue watching progress management.
- `settings_commands.rs`: Extension repository and provider configuration.

## Testing

```bash
cargo test --package desktop_tauri
```
