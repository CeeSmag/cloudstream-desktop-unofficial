# player_api

The contract between the app and a video player. `player_mpv` implements it; app services and UI
depend only on this crate.

- **Public API:** `VideoPlayer`, `PlaybackRequest`, `PlaybackStatus`, `PlayerError`.
- **Test:** `cargo test -p player_api`
