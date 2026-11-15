# player_mpv

`VideoPlayer` backed by libmpv, loaded at runtime (`libmpv-2.dll` / `libmpv.so` / `libmpv.dylib`),
so the app builds and starts without it and can report a clear error if it is missing.

- **Unsafe:** confined to `src/mpv_api.rs` (FFI); everything else is safe Rust.
- **Locating libmpv:** `CS_LIBMPV_PATH`, then the executable's folder, then the working folder.
- **Diagnostics:** 30 s after a stream loads, `playback_diagnostics` logs one `info` line (decoder,
  codec, frame rates, size, video output) and a `warn` if the video is decoded in software
  (`diagnostics_timer` decides when). A mpv log line that says the graphics device was lost or hardware
  decoding failed (`hwdec_problem`) is logged as a `warn` too. Both go to `player-logs.txt`.
- **Test:** `cargo test -p player_mpv`; the tests that need libmpv are ignored:
  `CS_LIBMPV_PATH=...\libmpv-2.dll cargo test -p player_mpv -- --ignored`
  (`t4_playback_diagnostics` takes about 35 s).
