# app_services

Use-case services the UI calls. Collaborators are trait objects or stores so services are
tested with fakes and wired to real implementations only in `app_shell`. The UI-facing bundle is
`UiServices`.

| Piece | Responsibility |
|---|---|
| `RepositoryService` | Add/remove repositories, list plugins, build catalog index, import discovered repos |
| `ExtensionService` | Install/uninstall `.cs3`, load into the gateway, trust flag, resident worker cap, provider list |
| `PluginIndex` | In-memory searchable index of plugins across repositories |
| `CatalogService` | Multi-provider search, home page shelves, title details |
| `BrowseService` (`BrowseClient`) | TMDB Discover browse (genre/year/rating filters) |
| `StreamResolver` | Title/episode → playable links (retry empty/timeout, extractor fallback, best-first sort) |
| `LinkSearch` | Streaming link-search poll protocol over the gateway |
| `StreamChecker` / `LinkHealth` | HTTP Range probes (run serially by `playback_session`; `check_worker_count` is exported but unused) |
| `PlaybackService` | Start/pause/seek/volume/cache limits, resume position, progress every 5 s, resume-pointer rules |
| `RoutedPlayer` / `RequestRouter` | Wrap any `VideoPlayer` so a router (e.g. stream proxy) can rewrite each `play` |
| `LibraryService` | Bookmarks, continue-watching (max 20), resume pointers |
| `SettingsService` | Typed settings (see below) |
| `clear_key` / `stream_label` | ClearKey DRM key normalization, pipe-URL header extraction, stream labels & kinds |
| `Clock` / `SystemClock` / `ServiceError` | Injectable wall clock and unified service error envelope |
| `measure_storage` / `clean_regenerable` | Data-folder size breakdown and regenerable cleanup |
| `CleanupScheduler` / `CleanupHandle` / `StorageCleanupRunner` | Single-worker post-Home + hourly hygiene, storage-cap enforcement, and manual cleanup (resets hourly cadence) |

### Settings (`SettingsService` + sibling modules)

One `impl SettingsService` per group: providers / debug / autoplay / skip lookup / sources-valid-from
(`settings_service.rs`); audio & subtitle languages (`language_settings.rs`, up to 3 preferred
subs); stream proxy; video cache size & ahead share; storage cap GB; plugin worker limit;
Cloudflare allowed hosts; home provider.

### Stream check

`StreamChecker::check` asks the host for the first bytes via `HttpProbe`.
`LinkHealth::Working` carries optional size and embedded subtitle language names from the head;
`Broken` is a log-ready reason. `check_worker_count` clamps `available_parallelism` to 2…12 (not called yet).

Test: `cargo test -p app_services`.
