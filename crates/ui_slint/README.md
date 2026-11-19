# ui_slint

The product UI (Slint 1.19, edition 1.19, Fluent dark style). `.slint` files in `ui/` draw; Rust in
`src/bridge/` connects them to `app_services` and never blocks the window: every slow call runs on a
worker thread and comes back as a `UiEvent` (`src/ui_events.rs`), drained by a 200 ms timer.

| Part | Responsibility |
|---|---|
| `bridge/app_start.rs` | window, wiring, plugin load, the UI tick and the 16 ms video frame timer |
| `bridge/*_callbacks.rs` | one file per screen's buttons (home, catalog, details, player, settings…) |
| `bridge/*_loading.rs`, `title_search.rs` | Home shelves, title pages, Browse (TMDB + provider match), search |
| `bridge/event_dispatch.rs` + `*_events.rs` | show background results; answers of replaced requests are dropped |
| `request_tickets.rs` | a ticket per request kind (home, search, browse, details, open) |
| `playback_watch.rs` | next source on failure (max 3), next episode on end |
| `focus_grid.rs`, `bridge/focus_*.rs` | remote / keyboard navigation: rows per screen, Enter / Back actions |
| `startup_progress.rs` | the start-up screen's text and bar |
| `bridge/poster_hydrate.rs` | artwork through `poster_artwork` (single rows updated, stale rows skipped) |
| `bridge/cloudflare_*.rs` | the Cloudflare banner (`ui/cloudflare_banner.slint`) |
| `bridge/extension_*.rs` | repositories, plugin catalog, installs, trust, removal (background, busy line) |

Navigation: the whole window is one `FocusScope`; keys a widget does not use bubble to it and go to
`focus_navigation` (Home, grids, title page; Settings hands Down to its side menu, Tab to controls).
Grid column counts and the top bar's narrow/wide layout come from Rust (`follow_window_width`):
reading a size in Slint to choose a layout makes a binding loop.

Notes: no `transform-scale` on tiles (FemtoVG drew nothing for it); no std `Slider` in the player
(dragging drops its binding; `DragBar` instead).

Test: `cargo test -p ui_slint` (pure logic). Screens: `slint-viewer --edition 1.19 --screenshot`.
