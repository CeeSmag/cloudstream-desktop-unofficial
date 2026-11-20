# player_api

Player-neutral contract: what the app asks of a video player and what it reports back.
`player_mpv` implements `VideoPlayer`; app services and UI depend only on this crate.

- **`VideoPlayer`:** play / pause / seek / stop / volume, cache limits, status, tracks,
  external subtitles (`add_subtitles`, `pending_subtitles`), preferred audio/subtitles, verbose
  logging, chapters, and (when embedded) surface size + `latest_frame`. A wrapper that forwards to
  another player (`RoutedPlayer` in `app_services`) must implement every method, including those
  with default bodies — a forgotten one silently keeps the default.
- **Public API:** see `src/lib.rs` (`VideoPlayer`, `PlaybackRequest`, `PlaybackStatus`,
  `PlayerError`, tracks, languages, frames, chapters, cache limits).
- **Test:** `cargo test -p player_api`
