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
| `bridge/stream_opening.rs`, `bridge/stream_events.rs`, `bridge/load_focus.rs`, `bridge/subtitle_*.rs` | open / refresh sources via `playback_session`; failure-refunded subtitle delivery & Sources/Subtitles Focus controls |
| `bridge/player_polling.rs` | playback status on the UI tick; frames on the 16 ms timer |
| `bridge/event_dispatch.rs` + `*_events.rs` | show background results; answers of replaced requests are dropped |
| `request_tickets.rs` | a ticket per request kind (home, search, browse, details, open, skips) |
| `playback_watch.rs` | next source on failure (max 3), next episode on end |
| `source_menu.rs`, `track_menu.rs` | player Sources (filters, sizes, subtitle tags) and track menus |
| `skip_segments.rs`, `seek_bar.rs`, `seek_scheduler.rs` | skip parts and prompt, seek bar extras, key seeks |
| `feedback_text.rs`, `check_text.rs` | player pop-ups, buffering text, the opening panel |
| `browse_filters.rs`, `pagination.rs`, `plugin_filter.rs`, `repository_filter.rs` | Browse filters/pagination, plugin & repository search/sort |
| `episode_groups.rs`, `posters.rs`, `shelf_pager.rs`, `bridge/home_expanded.rs` | Episode grouping, poster models, Home shelf memory rule (a collapsed row builds 10 posters; the expanded grid shows pages of 20 over `ShelfPager`) |
| `home_scroll.rs` | Home keeps its place: Home is rebuilt each time it shows, so its vertical and per-row sideways scroll live in the `HomeScroll` global and come back; only Home pressed on Home, or new shelves, go to the top (`tests/home_scroll_render_tests.rs`) |
| `focus_grid.rs`, `bridge/focus_*.rs`, `resume_hero.rs`, `performance_text.rs` | remote / keyboard navigation, Home banner, Settings "Performance & memory" texts |
| `startup_progress.rs`, `discovery_progress.rs` | the start-up screen's text/bar and background repo-discovery progress |
| `bridge/poster_hydrate.rs` | artwork through `poster_artwork` (single rows updated, stale rows skipped) |
| `cloudflare_prompts.rs`, `bridge/cloudflare_*.rs` | the Cloudflare banner: `PromptQueue` is a pure typed state machine (`NeedsPermission` > `Failed` > `Verifying` > `Verified`, deterministic expiry, same-tier cycling) projected one-way into `CloudflareBannerState` + `CloudflareBannerPhase` |
| `bridge/extension_*.rs`, `bridge/repo_list.rs`, `bridge/plugin_list.rs`, `repository_discovery.rs` | repositories tab, repo drill-down, aggregator repo discovery, plugins catalog, trust & removal |
| `webview_prompt.rs`, `bridge/webview_dialog.rs`, `ui/webview_dialog.slint` | Microsoft Edge WebView2 (Windows): read at every launch in the background; after Home, a missing runtime opens the `WebViewDialog` modal (Install / Not now; closing is not remembered, the next launch asks again), the install's progress, the Settings > General row, and the dialog when a Cloudflare check fails for want of it. Texts and the "open by itself" rule are pure (`webview_prompt.rs`); the status comes through `webview_runtime::StatusBoard`, the install through `InstallGate` |
| `text_sanitize.rs`, `windows_fonts.rs`, `ui/icon.png` | folds mathematical Unicode text, registers Windows fallback fonts, embeds window/taskbar icon |

Navigation: the whole window is one `FocusScope`; keys a widget does not use bubble to it and go to
`focus_navigation` (Home, grids, title page; Settings hands Down to its side menu, Tab to controls).
An open modal (`SecurityDialog`, `WebViewDialog`) takes every key first; "back" closes it.
Grid column counts and the top bar's narrow/wide layout come from Rust (`follow_window_width`):
reading a size in Slint to choose a layout makes a binding loop.

Notes: no `transform-scale` on tiles (FemtoVG drew nothing for it); no std `Slider` in the player
(dragging drops its binding; `DragBar` instead).
The player's Back button exits fullscreen and returns to a maximized details page.

The mouse pointer is hidden by Rust, not by a `mouse-cursor` binding: Slint hands the pointer to the
OS only while a mouse event is handled, and the controls hide on a timer with no event after it.
`bridge/cursor_sync.rs` sets it on the winit window (feature `pointer-hiding` = slint's
`unstable-winit-030`, on by default) when the player's controls change (`chrome-visibility-changed`)
and on every UI tick; the rule is `cursor_policy.rs`. Slint's `unstable-winit-030` forces X11 and
Wayland on, so the `--no-default-features` lint pass (`just clippy-strict`) leaves the feature out.

A click on the video goes through `click_sorter.rs` instead of Slint's `double-clicked`: the first
click waits 250 ms, a second one inside that is fullscreen (no pause), otherwise the frame tick
pauses. Pausing on each click paused and resumed the video on a double click.

Test: `cargo test -p ui_slint` (unit + headless render & window constraint tests in `tests/`).
