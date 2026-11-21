# ui_slint

The product UI (Slint 1.19, edition 1.19, Deep Obsidian Cinema theme + custom TV controls in
`ui/controls.slint` and vector `Path` icons in `ui/icons.slint`). `.slint` files in `ui/` draw; Rust in
`src/bridge/` connects them to `app_services` and never blocks the window: every slow call runs on a
worker thread (`background_work.rs`) and comes back as a `UiEvent` (`src/ui_events.rs`), drained by a
200 ms timer.

| Part | Responsibility |
|---|---|
| `bridge/app_start.rs` | window, wiring, plugin load, the UI tick and the 16 ms video frame timer |
| `bridge/*_callbacks.rs` | screen and settings buttons (home, catalog, details, player, preferences…) |
| `bridge/*_loading.rs`, `bridge/title_search.rs` | Home shelves, title pages, Browse (TMDB + provider match), search |
| `bridge/stream_opening.rs`, `bridge/stream_events.rs` | open / refresh sources via `playback_session`; progress on the player |
| `bridge/player_polling.rs` | playback status on the UI tick; frames on the 16 ms timer |
| `bridge/event_dispatch.rs` + `*_events.rs` | show background results; answers of replaced requests are dropped |
| `request_tickets.rs` | a ticket per request kind (home, search, browse, details, open, skips) |
| `playback_watch.rs` | next source on failure (max 3), next episode on end |
| `source_menu.rs`, `track_menu.rs` | player Sources (filters, sizes, subtitle tags) and track menus |
| `skip_segments.rs`, `seek_bar.rs`, `seek_scheduler.rs` | skip parts and prompt, seek bar extras, key seeks |
| `feedback_text.rs`, `check_text.rs` | player pop-ups, buffering text, the opening panel |
| `browse_filters.rs`, `pagination.rs`, `plugin_filter.rs`, `repository_filter.rs` | Browse filters/pagination, plugin & repository search/sort |
| `episode_groups.rs`, `posters.rs`, `bridge/home_expanded.rs` | Episode season/chunk grouping, Slint poster models, Home expanded shelf grid |
| `focus_grid.rs`, `bridge/focus_*.rs`, `resume_hero.rs` | remote / keyboard navigation, Home banner |
| `startup_progress.rs`, `discovery_progress.rs` | the start-up screen's text/bar and background repo-discovery progress |
| `bridge/poster_hydrate.rs` | artwork through `poster_artwork` (single rows updated, stale rows skipped) |
| `bridge/cloudflare_*.rs` | the Cloudflare banner (`ui/cloudflare_banner.slint`) |
| `bridge/extension_*.rs`, `bridge/repo_list.rs`, `bridge/plugin_list.rs`, `repository_discovery.rs` | repositories tab, repo drill-down, aggregator repo discovery, plugins catalog, trust & removal |
| `text_sanitize.rs`, `windows_fonts.rs`, `ui/icon.png` | folds mathematical Unicode text, registers Windows fallback fonts, embeds window/taskbar icon |

Navigation: the whole window is one `FocusScope`; keys a widget does not use bubble to it and go to
`focus_navigation` (Home, grids, title page; Settings hands Down to its side menu, Tab to controls).
Grid column counts and the top bar's narrow/wide layout come from Rust (`follow_window_width`):
reading a size in Slint to choose a layout makes a binding loop.

Notes: no `transform-scale` on tiles (FemtoVG drew nothing for it); no std `Slider` in the player
(dragging drops its binding; `DragBar` instead).
The player's Back button exits fullscreen and returns to a maximized details page.

Test: `cargo test -p ui_slint` (unit + headless render & window constraint tests in `tests/`).
