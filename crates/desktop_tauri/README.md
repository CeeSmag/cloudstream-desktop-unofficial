# desktop_tauri

Tauri v2 application shell providing a modern web UI overlay over hardware-accelerated libmpv video playback.

## Purpose

Hosts the React/Tailwind frontend in an opaque window. libmpv renders frames into memory; the page draws them on a canvas and layers the controls on top.

## Public Architecture

- `main.rs`: Entry point initializing `AppPaths`, logging, services, and native `wid` window handle binding.
- `video_commands.rs`: Delivers mpv's frames to the page as binary RGBA (the window itself is opaque).
- `catalog_commands.rs`: IPC commands for provider discovery, home shelves, search, and details.
- `playback_commands.rs`: IPC commands for stream resolution, player state, track selection, and seek.
- `library_commands.rs`: Bookmarks and continue watching progress management.
- `settings_commands.rs`: Extension repository and provider configuration.

## Testing

```bash
cargo test --package desktop_tauri
```
