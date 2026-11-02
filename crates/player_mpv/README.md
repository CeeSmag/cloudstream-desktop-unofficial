# player_mpv

`VideoPlayer` backed by libmpv, loaded at runtime (`libmpv-2.dll` / `libmpv.so` / `libmpv.dylib`),
so the app builds and starts without it and can report a clear error if it is missing.

- **Unsafe:** confined to `src/mpv_api.rs` (FFI); everything else is safe Rust.
- **Locating libmpv:** `CS_LIBMPV_PATH`, then the executable's folder, then the working folder.
- **Test:** `cargo test -p player_mpv`; the headless playback test needs libmpv and is ignored:
  `CS_LIBMPV_PATH=...\libmpv-2.dll cargo test -p player_mpv -- --ignored`
