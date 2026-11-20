# player_mpv

`VideoPlayer` backed by libmpv, loaded at runtime (`libmpv-2.dll` / `mpv-2.dll`, `libmpv.so.2` /
`libmpv.so`, `libmpv.dylib`), so the app builds and starts without it and can report a clear error
if it is missing.

- **Unsafe:** confined to the libmpv FFI modules (`src/mpv_api.rs`, `src/mpv_api/properties.rs`,
  `src/mpv_log.rs`, `src/mpv_render_api.rs`); everything else is safe Rust.
- **Locating libmpv:** `CS_LIBMPV_PATH`, then the executable's folder, then the working folder, and
  (on Linux/macOS) the bare library names via the system dynamic loader.
- **Diagnostics:** 30 s after a stream loads, `playback_diagnostics` logs one `info` line (decoder,
  codec, frame rates, size, video output) and a `warn` if the video is decoded in software
  (`diagnostics_timer` decides when). A mpv log line that says the graphics device was lost or hardware
  decoding failed (`hwdec_problem`) is logged as a `warn` too. Both go to `player-logs.jsonl`.
- **Cache:** `cache_limits` sets the bytes kept ahead of and behind the playing position
  (`VideoPlayer::set_cache_limits`, any time) and, per stream, how far ahead mpv reads: one minute for
  HLS/DASH (many requests, HTTP 429 risk), ten hours for a plain file (only the byte limit counts).
  Seen with the real libmpv: a lowered behind limit trims at once, a lowered ahead limit keeps what was
  read, a raised one is read up to once playback goes on (`cache_limit_live_tests.rs`, needs
  `CS_CACHE_VIDEO`).
- **Test:** `cargo test -p player_mpv`; the tests that need libmpv are ignored:
  `CS_LIBMPV_PATH=...\libmpv-2.dll cargo test -p player_mpv -- --ignored`
  (`t4_playback_diagnostics` takes about 35 s).
