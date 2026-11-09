# AGENTS.md — Developer & AI Agent Codebase Guide

This document is the verified architectural index, navigation map, and modification handbook for the **CloudStream Rust rewrite (`cloudstream-rs`)**. Every symbol, file path, trait method, DTO, IPC command, and table schema documented here has been verified against the active source code.

---

## Table of Contents

1. [Architecture & Workspace Overview](#1-architecture--workspace-overview)
2. [Crate Verification & Package Map](#2-crate-verification--package-map)
   - [Chunk 1: Foundational Models & Trait Contracts](#chunk-1-foundational-models--trait-contracts)
   - [Chunk 2: Persistence & Cache Infrastructure](#chunk-2-persistence--cache-infrastructure)
   - [Chunk 3: Extension Subsystem & Sidecar Protocol](#chunk-3-extension-subsystem--sidecar-protocol)
   - [Chunk 4: Parsing & Extractor Stubs](#chunk-4-parsing--extractor-stubs)
   - [Chunk 5: Video Player Engine (libmpv)](#chunk-5-video-player-engine-libmpv)
   - [Chunk 6: Application Services Layer](#chunk-6-application-services-layer)
   - [Chunk 7: Shells & UI Frontends](#chunk-7-shells--ui-frontends)
   - [Chunk 8: Scripts, Tooling & External Repositories](#chunk-8-scripts-tooling--external-repositories)
3. [Subsystem Lifecycles & Data Flow Diagrams](#3-subsystem-lifecycles--data-flow-diagrams)
   - [Extension Execution & Crash Recovery](#extension-execution--crash-recovery)
   - [Video Stream Resolution, Check Gate & Fallback](#video-stream-resolution-check-gate--fallback)
   - [Subtitle Ingestion & Local Normalization](#subtitle-ingestion--local-normalization)
4. [Tauri IPC Interface (56 Commands Reference)](#4-tauri-ipc-interface-56-commands-reference)
5. [Code Navigation Guide ("Where is X?")](#5-code-navigation-guide-where-is-x)
6. [Step-by-Step Modification Recipes ("How do I do X?")](#6-step-by-step-modification-recipes-how-do-i-do-x)
   - [Recipe 1: Adding a new Tauri IPC command (Rust + React)](#recipe-1-adding-a-new-tauri-ipc-command-rust--react)
   - [Recipe 2: Modifying or adding a React frontend component](#recipe-2-modifying-or-adding-a-react-frontend-component)
   - [Recipe 3: Adding or updating an Application Service](#recipe-3-adding-or-updating-an-application-service)
   - [Recipe 4: Adding a database migration and store method](#recipe-4-adding-a-database-migration-and-store-method)
   - [Recipe 5: Modifying stream checking or playback fallback](#recipe-5-modifying-stream-checking-or-playback-fallback)
   - [Recipe 6: Writing concurrent code and mandatory race tests](#recipe-6-writing-concurrent-code-and-mandatory-race-tests)
7. [Invariants, Quality Gates & Project Rules (RULES.md)](#7-invariants-quality-gates--project-rules-rulesmd)
8. [Developer Commands & Environment Variables](#8-developer-commands--environment-variables)

---

## 1. Architecture & Workspace Overview

`cloudstream-rs` is a high-performance desktop rewrite of **CloudStream** (the Android streaming application).

### Core Pillars
- **Extension Parity**: Runs real CloudStream `.cs3` plugins via an out-of-process JVM sidecar (`sidecar/cs-sidecar`) executing converted DEX bytecode against Android stubs behind the `ProviderGateway` trait.
- **Player Backend**: `libmpv` is the sole video player engine, loaded dynamically at runtime (`libmpv-2.dll` on Windows, `.so.2` on Linux, `.dylib` on macOS).
- **Decoupled One-Way Flow**: `ui -> services -> traits <- implementations`. Concrete implementations are only wired in `app_shell`.
- **Fault-Tolerant Resilience**: No unhandled panics (`unwrap`, `expect`, `panic!`, `indexing_slicing` denied). Fallible paths have explicit fallbacks or structured errors.
- **Dual Application Frontends**:
  1. `desktop_tauri`: Tauri v2 application shell hosting a React 18 + TypeScript + Vite + Tailwind CSS frontend with software-rendered libmpv frames over an HTML5 `<canvas>`.
  2. `app_shell` / `ui_app`: Native desktop shell running an immediate-mode `egui` GUI with blurred glass shaders and embedded mpv textures.

---

## 2. Crate Verification & Package Map

### Chunk 1: Foundational Models & Trait Contracts

#### `crates/core_model`
- **Location**: `crates/core_model/`
- **Dependencies**: `serde`
- **Responsibility**: Pure domain types shared across the workspace. Zero I/O, zero network, fast compilation.
- **Verified Public Types**:
  - `Bookmark`: `provider`, `url`, `title: name`, `poster_url`, `media_type`, `added_at: i64`.
  - `DiscoveredRepository`: `name`, `url`.
  - `HomePage`: `items: Vec<HomePageSection>`.
  - `HomePageSection`: `name: String`, `items: Vec<SearchResult>`.
  - `InstalledExtension`: `file_name`, `internal_name`, `repository_url: Option<String>`, `version: i32`, `trusted: bool`, `icon_url: Option<String>`.
  - `LinkSearchCursor`: `links: usize`, `subtitles: usize`.
  - `LinkSearchUpdate`: `links: Vec<PlayableLink>`, `subtitles: Vec<SubtitleTrack>`, `is_done: bool`, `next_link: usize`, `next_subtitle: usize`, `cursor() -> LinkSearchCursor`.
  - `LoadDetails`: Title `name`, `url`, `api_name`, `media_type`, `data_url`, `poster_url`, `year`, `plot`, `tags: Option<Vec<String>>`, `episodes: Vec<EpisodeInfo>`, `background_poster_url`, `rating: Option<i32>`, `duration: Option<i32>`, `actors: Vec<CastMember>`.
  - `EpisodeInfo`: `data: String` (provider payload), `name`, `season: Option<i32>`, `episode: Option<i32>`, `poster_url`, `description`.
  - `CastMember`: `name: String`, `image: Option<String>`, `role: Option<String>`.
  - `PlayableLink`: `source: String`, `name: String`, `url: String`, `referer: Option<String>`, `quality: Option<i32>`, `headers: Option<HashMap<String, String>>`, `is_m3u8: Option<bool>`.
  - `PlayableLinks`: `success: bool`, `links: Vec<PlayableLink>`, `subtitles: Vec<SubtitleTrack>`.
  - `SubtitleTrack`: Provider subtitle item (`lang: String`, `url: String`).
  - `PluginListing`: `name`, `internal_name`, `version: i32`, `authors: Vec<String>`, `description: Option<String>`, `url: String`, `icon_url: Option<String>`.
  - `ProviderInfo`: `name: String`, `main_url: String`, `lang: String`, `types: Vec<String>`, `has_main_page: bool`, `main_page: Vec<MainPageEntry>`.
  - `MainPageEntry`: `name: String`, `data: String`, `horizontal_images: bool`.
  - `RepositoryEntry`: `url: String`, `name: String`, `icon_url: Option<String>`.
  - `SearchResult`: `name`, `url`, `poster_url`, `api_name`, `media_type`, `year: Option<i32>`.
  - `SearchPage`: `items: Vec<SearchResult>`.
  - `WatchProgress`: `provider`, `media_key`, `title`, `detail_url`, `poster_url`, `position_ms: i64`, `duration_ms: i64`, `updated_at_ms: i64`, `season: Option<i32>`, `episode: Option<i32>`, `fraction_watched() -> f32`.
  - `ResumePointer`: `provider: String`, `detail_url: String`, `media_key: String`, `title: String`, `poster_url: Option<String>`, `season: Option<i32>`, `episode: Option<i32>`, `updated_at_ms: i64`.
  - `stable_key(text: &str) -> String`: Deterministic 16-character hex hash based on FNV constants for safe file/cache keys.

#### `crates/extension_api`
- **Location**: `crates/extension_api/`
- **Dependencies**: `core_model`, `thiserror`
- **Responsibility**: Trait definitions and error types between the app core and extension runtimes.
- **Verified Trait `ProviderGateway: Send + Sync`**:
  - `load_extension(path: &Path, trusted: bool) -> Result<Vec<ProviderInfo>, GatewayError>`
  - `search(provider: &str, query: &str, page: u32) -> Result<SearchPage, GatewayError>`
  - `main_page(provider: &str, page: u32, entry: &MainPageEntry) -> Result<Option<HomePage>, GatewayError>`
  - `load(provider: &str, url: &str) -> Result<Option<LoadDetails>, GatewayError>`
  - `load_links(provider: &str, data: &str) -> Result<PlayableLinks, GatewayError>`
  - `start_link_search(provider: &str, data: &str) -> Result<String, GatewayError>`
  - `poll_link_search(id: &str, cursor: LinkSearchCursor, wait: Duration) -> Result<LinkSearchUpdate, GatewayError>`
  - `cancel_link_search(id: &str) -> Result<(), GatewayError>`
  - `extract(url: &str, referer: Option<&str>) -> Result<PlayableLinks, GatewayError>`
  - `take_added_repositories() -> Result<Vec<DiscoveredRepository>, GatewayError>`
  - `set_debug_logging(enabled: bool) -> Result<(), GatewayError>`
- **Verified Error `GatewayError`**:
  - `Unavailable(String)`: Runtime not started or missing.
  - `Timeout { method: String, seconds: u64 }`: Call deadline exceeded.
  - `Transport(String)`: Process crash, closed pipe, or I/O failure.
  - `Remote { kind: String, message: String }`: Extension or runtime exception.
  - `Decode { method: String, reason: String }`: Reply schema mismatch.

#### `crates/player_api`
- **Location**: `crates/player_api/`
- **Dependencies**: `thiserror`
- **Responsibility**: Decoupled player contract between application services and video player engines.
- **Verified Trait `VideoPlayer: Send + Sync`**:
  - `play(request: &PlaybackRequest) -> Result<(), PlayerError>`
  - `set_paused(paused: bool) -> Result<(), PlayerError>`
  - `seek_to(position_ms: i64) -> Result<(), PlayerError>`
  - `stop() -> Result<(), PlayerError>`
  - `set_volume(percent: u8) -> Result<(), PlayerError>`
  - `status() -> PlaybackStatus`
  - `tracks() -> Vec<MediaTrack>`
  - `select_track(kind: TrackKind, id: Option<i64>) -> Result<(), PlayerError>`
  - `add_subtitles(subtitles: &[SubtitleSource])`
  - `set_preferred_audio(language: Option<Language>)`
  - `set_preferred_subtitles(languages: &[Language])`
  - `is_embedded() -> bool`
  - `set_surface_size(width: u32, height: u32)`
  - `latest_frame(last_seen_serial: u64) -> Option<Arc<VideoFrame>>`
- **Verified Types**:
  - `TrackKind`: Enum with `Audio`, `Subtitle`.
  - `SubtitleSource`: Local file path/URL and label for external subtitle attachment.
  - `PlaybackRequest`: Title, URL, referer, custom HTTP headers, start position in ms, and initial subtitles.
  - `PlaybackStatus`: State (`PlaybackState`: `Idle`, `Loading`, `Playing`, `Paused`, `Ended`, `Failed`), duration, position, volume.
  - `Language`: ISO 639-1 / 639-2 codes, with helpers `all_languages()`, `find_language()`, and `ENGLISH`.
  - `VideoFrame`: Width, height, monotonically increasing `serial: u64`, and RGBA byte buffer.
  - `PlayerError`: `Unavailable(String)`, `Command { command: String, reason: String }`.

---

### Chunk 2: Persistence & Cache Infrastructure

#### `crates/storage`
- **Location**: `crates/storage/`
- **Dependencies**: `core_model`, `rusqlite = "0.37"`, `thiserror`, `tracing`
- **Database Path**: `<data_dir>/cloudstream.db` (enables SQLite WAL mode).
- **Verified Schema Migrations (`schema_migrations.rs`)**:
  - *Version 1*:
    - `settings`: `(key TEXT PRIMARY KEY, value TEXT NOT NULL)`
    - `repositories`: `(url TEXT PRIMARY KEY, name TEXT NOT NULL, added_at INTEGER NOT NULL)`
    - `installed_extensions`: `(file_name TEXT PRIMARY KEY, internal_name TEXT NOT NULL, repository_url TEXT, version INTEGER NOT NULL, trusted INTEGER NOT NULL)`
    - `bookmarks`: `(provider TEXT NOT NULL, url TEXT NOT NULL, name TEXT NOT NULL, poster_url TEXT, media_type TEXT, added_at INTEGER NOT NULL, PRIMARY KEY (provider, url))`
    - `watch_progress`: `(provider TEXT NOT NULL, media_key TEXT NOT NULL, title TEXT NOT NULL, detail_url TEXT NOT NULL, poster_url TEXT, position_ms INTEGER NOT NULL, duration_ms INTEGER NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY (provider, media_key))`
  - *Version 2*:
    - `ALTER TABLE repositories ADD COLUMN icon_url TEXT;`
    - `ALTER TABLE installed_extensions ADD COLUMN icon_url TEXT;`
  - *Version 3*:
    - `CREATE TABLE resume_pointers (provider TEXT NOT NULL, detail_url TEXT NOT NULL, media_key TEXT NOT NULL, title TEXT NOT NULL, poster_url TEXT, season INTEGER, episode INTEGER, updated_at INTEGER NOT NULL, PRIMARY KEY (provider, detail_url));`
    - Migrates latest watched episode per show from `watch_progress` into `resume_pointers`.
- **Stores**:
  - `SettingsStore`: `get(key)`, `set(key, value)`, `remove(key)`.
  - `RepositoryStore`: `add(entry, added_at)`, `remove(url)`, `list()`, `set_icon_url(url, icon_url)`.
  - `InstalledExtensionStore`: `add(ext)`, `remove(file_name)`, `list()`, `set_trusted(file_name, trusted)`, `set_icon_url(file_name, icon_url)`.
  - `BookmarkStore`: `add(bookmark)`, `remove(provider, url)`, `contains(provider, url)`, `list()`.
  - `WatchProgressStore`: `save(progress)`, `get(provider, media_key)`, `recent(limit)`, `remove(provider, media_key)`, `episodes_for_show(provider, detail_url)`, `remove_for_show(provider, detail_url)`, `all_media_keys()`, `clear()`.
  - `ResumeStore`: `save(pointer)`, `get(provider, detail_url)`, `remove(provider, detail_url)`, `recent(limit)`, `all()`, `clear()`.
  - All stores wrap queries over `Arc<Database>`.
- **Error**: `StorageError` (`Io { path, source }`, `Sql { operation, source }`).
- **Clock Rule**: Callers pass timestamps explicitly; stores do not query the system clock directly.

#### `crates/subtitle_store`
- **Location**: `crates/subtitle_store/`
- **Dependencies**: `core_model`, `flate2 = "1"`, `serde`, `serde_json`, `thiserror`, `tracing`, `ureq = "3"`
- **Storage Location**: `<data_dir>/subtitles/<url_hash>.vtt` (default retention: 7 days).
- **Verified Modules & Types**:
  - `normalize_subtitle.rs`: `normalize_subtitle(bytes) -> Result<NormalizedSubtitle, NormalizeError>`. Unpacks `.zip` archives, decompresses `.gz` streams, strips UTF-8 BOM, decodes UTF-16 LE/BE and Windows-1252 to UTF-8, rejects HTML error/challenge responses, parses SRT/VTT.
  - `hls_subtitles.rs`: `flatten_playlist(text, base_url, fetcher)` flattens segmented M3U8 subtitle playlists into a unified WebVTT stream.
  - `subtitle_cache.rs`: `SubtitleCache::new(dir, ttl)` manages on-disk storage with LRU and TTL pruning (`prune() -> usize`). Stores files as `<dir>/<sha256>.vtt`.
  - `subtitle_fetcher.rs`: Trait `SubtitleFetcher` and `UreqSubtitleFetcher` enforcing size limits and provider request headers.
  - `subtitle_loader.rs`: `SubtitleLoader` coordinates cache lookup -> HTTP fetch -> normalize -> disk save. `LoadOptions` (default 3 attempts, 400ms exponential backoff).
  - `subtitle_prefetch.rs`: `SubtitlePrefetch` multi-threaded background fetcher prioritizing tracks matching the user's preferred language rankings (`LanguageRank`), stoppable via `KeepGoing`.

#### `crates/title_cache`
- **Location**: `crates/title_cache/`
- **Dependencies**: `core_model`, `serde`, `serde_json`, `thiserror`, `tracing`
- **Storage Location**: `<data_dir>/sources/<hash of provider+media_key>.json`
- **Verified Limits (`cache_limits.rs`)**:
  - `fresh_duration`: 20 minutes (links tried immediately without re-searching).
  - `stale_duration`: 24 hours (best 3 links tested with 3s probe timeout).
  - `max_titles`: 50 entries (LRU eviction).
  - `max_links_per_title`: 200 links.
  - `max_file_size`: 1 MB.
- **Provider switch**: `TitleCache::invalidate_now()` / `set_valid_from(secs)` make every save from before that time unusable (`Miss::Invalidated`, file removed by the next `prune`, counted in `PruneReport::invalidated`); notes (`mark_broken`, ...) never revive such a file. `desktop_tauri::provider_switch::apply_switch` (command `provider_switched`) calls it when the viewer picks a different provider, and stores the time in the `sources_valid_from` setting, which `sources_cleanup::open_title_cache` restores at start. `sources_cleanup::spawn_cleanup` prunes at start and then hourly while the app runs.
- **Atomic File Operations**: Writes to temporary `.tmp` files and renames into place, preventing corruption on app crash.
- **Verified Types & Methods**:
  - `TitleCache`: `load(key) -> Lookup`, `save(key, title) -> Result<SaveReport, TitleCacheError>`, `update(key, fn)`, `mark_broken(key, url)`, `forget(key)`, `clear()`, `prune() -> PruneReport`.
  - `Lookup`: `Hit(CacheHit)` or `Miss(Miss)`. `CacheHit` carries `CachedTitle`, `Freshness` (`Fresh`, `Stale`), and `age`.
  - `CachedTitle`: `complete: bool`, `links: Vec<CachedLink>`, `last_good: Option<String>`. `playing_order() -> Vec<&CachedLink>`.
  - `CachedLink`: `link: PlayableLink`, `health: LinkHealth` (`Working`, `Broken`, `Unchecked`).
  - `redact_url(url) -> String`: Sanitizes tokens into `host#fingerprint` for safe logging.

#### `crates/skip_times`
- **Location**: `crates/skip_times/`
- **Dependencies**: `serde`, `serde_json`, `thiserror`, `tracing`, `ureq = "3"`
- **Responsibility**: Finds the intro, recap and credits of an episode or film. Providers do not carry these times; like the reference app it asks community databases by the title's ids: AniSkip (anime, MyAnimeList id), TheIntroDB (TMDb or IMDb id), IntroDB (IMDb id). First database that knows wins.
- **Types**: `SkipLookup::find(&SkipQuery) -> Result<Vec<SkipSegment>, SkipError>` (answers remembered per run, at most 64; errors are not remembered), `ExternalIds::from_sync(sync_data, unique_url)` (validates ids so they are safe in a URL), `SkipFetcher` trait with `UreqSkipFetcher`.
- **Plumbing**: `core_model::LoadDetails.sync_data` / `unique_url` come from the sidecar's `load` reply; `desktop_tauri::catalog_commands::map_media_details` turns them into `imdb_id` / `tmdb_id` / `mal_id` on the title page; `skip_commands::get_skip_segments` runs the lookup (switch: setting `skip_lookup`).

---

### Chunk 3: Extension Subsystem & Sidecar Protocol

#### `crates/extension_runtime`
- **Location**: `crates/extension_runtime/`
- **Dependencies**: `core_model`, `extension_api`, `serde`, `serde_json`, `tracing`
- **Binary**: `fake_sidecar` (`src/bin/fake_sidecar.rs`, enabled via `test-support` feature).
- **Verified Modules & Constants**:
  - `call_timeouts.rs`: `PROVIDER_CALL_TIMEOUT` (60s), `LINK_RESOLVE_TIMEOUT` (150s), `EXTENSION_LOAD_TIMEOUT` (180s for DEX bytecode conversion).
  - `sidecar_launch_config.rs`: `SidecarLaunchConfig` configures `CS_SIDECAR_HOME`, `CS_SIDECAR_JAVA_HOME`, data dir (`-Dcloudstream.data.dir=...`), UTF-8 I/O encoding, and Windows root certificates (`-Djavax.net.ssl.trustStoreType=Windows-ROOT`).
  - `sidecar_process_transport.rs`: `SidecarProcessTransport` manages child process over stdin/stdout JSON-lines RPC (`CREATE_NO_WINDOW` on Windows). On crash or timeout, kills process and restarts on next call, replaying `startup_calls` (`remember_for_restart`).
  - `rpc_transport.rs`: Trait `RpcTransport` (`call`, `remember_for_restart`, `remember_setting_for_restart`).
  - `rpc_provider_gateway.rs`: `RpcProviderGateway<T>` implements `ProviderGateway`. Flags slow calls (>10s) and filters repetitive background polls (`main_page`, `take_added_repositories`, `poll_link_search`) to debug logs.
  - `fake_link_search.rs` & `fake_search_runtime.rs`: Deterministic doubles for streaming link search protocol testing.

#### `crates/extension_repo`
- **Location**: `crates/extension_repo/`
- **Dependencies**: `core_model`, `serde`, `serde_json`, `thiserror`, `tracing`, `ureq = "3"`
- **Verified Modules & Types**:
  - `repository_client.rs`: `RepositoryClient<F: HttpFetcher>` fetches manifests (`repo.json`) and plugin listings (`plugins.json`).
  - `extension_installer.rs`: `ExtensionInstaller<F: HttpFetcher>` downloads `.cs3` plugins to `<data_dir>/extensions/` using atomic `.cs3.part` writes and path sanitization (`safe_file_name.rs`).
  - `http_probe.rs`: Trait `HttpProbe` and `UreqProbe` (8s timeout, probes first 32 KB via HTTP `Range: bytes=0-32767` to verify video/playlist streams). Returns `ProbeResponse` (`status`, `content_type`, `head`, `total_bytes`).
  - `repo_error.rs`: `RepoError` (`Download { url, reason }`, `Invalid { url, reason }`, `Disk { path, source }`, `UnsafeName(String)`).

#### `sidecar/cs-sidecar`
- **Location**: `sidecar/cs-sidecar/`
- **Build System**: Gradle 8.x + Kotlin 2.1 (JVM Target 21).
- **Compatibility Bridge**: Converts Android DEX bytecode to JVM bytecode via `dex2jar` / `plugin-runtime` and provides fake Android environment classes (`android-stubs`).
- **Verified Kotlin Modules (`src/main/kotlin/`)**:
  - `sidecar/SidecarMain.kt`: Entry point reading stdin JSON lines and dispatching RPC messages. Exits when stdin closes (plugin threads would otherwise keep the JVM alive, holding plugin files open); on Windows the Rust transport also ends the whole `cs-sidecar.bat` -> `java.exe` process tree when it discards the sidecar.
  - `sidecar/ProviderService.kt`: Executes extension calls (`search`, `main_page`, `load`, `load_links`, `extract`).
  - `sidecar/LinkSearches.kt`: Coordinates asynchronous streaming link searches and channel buffers.
  - `sidecar/AddedRepositories.kt`: Buffers repositories discovered/added dynamically by extensions.
  - `network/CloudflareKiller.kt`: Intercepts and resolves Cloudflare challenges.
  - `syncproviders/`: Stubs for CloudStream account and sync providers (`AccountManager`, `SyncStubs`, `SimklApi`).
- **Verified Tests (`src/test/kotlin/`)**:
  - `sidecar/LinkSearchesTest.kt`: Multi-threaded race tests verifying streaming link search concurrency.
- **Build Script**: `scripts/setup_sidecar.ps1` clones the pinned reference client into `C:\cs3d\repo` and compiles the distribution.

#### `spikes/wasmtime_host`
- **Location**: `spikes/wasmtime_host/`
- **Status**: Feasibility spike testing Kotlin/Wasm GC compilation running in Wasmtime 50.0.1. Retained as reference for future pure-native WASM extension runtime.

---

### Chunk 4: Parsing & Extractor Stubs

#### `crates/network_client`, `crates/html_parsing`, `crates/extractors`
- **Locations**: `crates/network_client/`, `crates/html_parsing/`, `crates/extractors/`
- **Status**: Minimal stub crates configured with `#![forbid(unsafe_code)]`.
- **Purpose**: Architectural placeholders reserved for native Rust extractors and direct web resolvers when moving extractors out of the JVM sidecar.

---

### Chunk 5: Video Player Engine (libmpv)

#### `crates/player_mpv`
- **Location**: `crates/player_mpv/`
- **Dependencies**: `player_api`, `libloading = "0.8"`, `tracing`
- **Unsafe Scope**: Confined strictly to `src/mpv_api.rs` with `// SAFETY:` justifications.
- **Dynamic Locator (`libmpv_locator.rs`)**: Searches `CS_LIBMPV_PATH`, then the directory of `current_exe()`, then `current_dir()`. Looks for `libmpv-2.dll` / `mpv-2.dll` on Windows.
- **Player Configuration (`player_configuration.rs`)**:
  - `PlayerOptions`: `headless: bool` (`vo=null, ao=null`), `embedded: bool` (`vo=libmpv`), `wid: Option<i64>` (`vo=gpu, hwdec=auto-safe`).
  - Sets common options: `idle=yes`, `keep-open=no`, `osc=no`, `input-default-bindings=no`, `hwdec=auto-copy-safe`, `ytdl=no`, `alang=eng,en`.
  - Debugging: setting `CS_MPV_LOG` passes `terminal=yes` and `msg-level=all=v` for native mpv console diagnostics.
- **Rendering Modes**:
  1. *Embedded Software Renderer*: Uses libmpv's render API (`vo=libmpv`) to blit frames into an RGBA memory buffer (`VideoFrame`). Enables frontend canvas/glass overlay.
  2. *Window Attachment (`vo=gpu`)*: Attaches directly to a native window handle (`wid: Option<i64>`).
  3. *Headless*: Plays audio/timing without video output for T4 automated integration testing.
- **Track & Subtitle Management**:
  - `preferred_audio.rs`: Automatically activates audio tracks matching ISO language preference (`AudioChoice::Switch { id }`).
  - `preferred_subtitles.rs`: Selects preferred subtitle languages once streams load without overriding manual user track picks, guarded by stream generation counters (`chosen_for`, `changed_by_viewer_for`).
  - `pending_subtitles.rs`: Queues subtitle tracks arriving asynchronously while mpv is initializing.

---

### Chunk 6: Application Services Layer

#### `crates/app_services`
- **Location**: `crates/app_services/`
- **Dependencies**: `core_model`, `extension_api`, `extension_repo`, `player_api`, `storage`, `thiserror`, `tracing`
- **Verified Public Services**:
  - `CatalogService`: `search(providers, query) -> Vec<ProviderSearchOutcome>`, `home(provider, entry) -> Result<Option<HomePage>>`, `details(provider, url) -> Result<LoadDetails>`.
  - `ExtensionService`: `install(listing, repo_url)`, `uninstall(file_name)`, `set_trusted(file_name, trusted)`, `load_installed(progress_cb)`, `providers()`, `installed()`, `set_debug_logging(enabled)`.
  - `RepositoryService`: `repositories()`, `add(url)`, `remove(url)`, `plugins(url)`, `import_discovered()`, `refresh_icons()`.
  - `PluginIndex`: In-memory searchable index of all plugins across all registered repositories.
  - `StreamResolver`: Initiates streaming link resolution via `ProviderGateway::start_link_search` with automatic retry logic; sorts candidates using `playable_links_best_first` and `link_rank`.
  - `StreamChecker`: Concurrently probes video streams via 12 worker threads (`WORKERS = 12`) using HTTP Range probes (`Range: bytes=0-32767`). Evaluates `LinkHealth` (`Working { size_bytes, subtitles }`, `Broken(String)`).
  - `CheckQueue`: Thread-safe priority queue sorting streams by resolution/quality (`push`, `close`, `stop`, `next`).
  - `PlaybackService`: Coordinates `VideoPlayer`, `WatchProgressStore`, and `ResumeStore`. Manages `start()`, `start_at()`, `resume_position_ms()`, `set_paused()`, `seek_to()`, `set_volume()`, `tracks()`, `select_track()`, `latest_frame()`. Progress automatically updates every 5,000 ms (`SAVE_EVERY_MS = 5_000`). Advances resume pointer to `next_episode` when progress reaches ≥ 90% (`NEXT_WATCH_EPISODE_PERCENTAGE = 0.90`), or removes pointer if last episode/movie.
  - `LibraryService`: Bookmarks, continue-watching, and resume pointers. `continue_watching()` returns one card per show, newest first, carrying season/episode and progress from `ResumeStore`. `resume_pointer(provider, detail_url)` finds resume position for details page. `remove_continue_watching(provider, detail_url)` and `clear_continue_watching()` remove resume pointers and episode progress rows. Max 20 items in continue-watching.
  - `SettingsService`: Typed getters/setters for disabled providers, audio language, subtitle language list (up to 3: `MAX_PREFERRED_SUBTITLES = 3`), auto-subtitles, and debug logging.
  - `Clock` Trait: `SystemClock` for production, mockable clock for deterministic tests.
  - `ServiceError`: Transparent error envelope wrapping `GatewayError`, `RepoError`, `StorageError`, `PlayerError`, and `NotFound(String)`.

---

### Chunk 7: Shells & UI Frontends

#### `crates/app_shell`
- **Location**: `crates/app_shell/`
- **Binary**: `cloudstream` (`src/main.rs`)
- **Key Modules & Paths**:
  - `app_paths.rs`: Locates data directory: `CS_DATA_DIR` -> `%LOCALAPPDATA%\CloudStreamRs` (Windows) -> `~/.local/share/cloudstream-rs` (Linux). Holds `database_file()`, `extensions_dir()`, `log_file()`.
  - `logging_setup.rs`: Initializes `tracing-subscriber` logging to `<data>/logs/cloudstream.log` and reloadable debug/problem category layers in `<data>/logs/debug/` (`DebugLogs`).
  - `sidecar_discovery.rs`: Locates JVM sidecar via `CS_SIDECAR_HOME` or default `C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar`. Detects JDK 21 in `C:\Program Files\Java`, `Eclipse Adoptium`, `Microsoft`.
  - `data_wipe.rs`: the fallback of "Delete all saved data" when the helper cannot start. `request_wipe` writes `delete_all_data.pending`; `wipe_if_requested` (called first thing in both `main`s, before logging) removes everything in the data folder except the marker and the portable exe's `runtime-<version>` folders, retrying for about 10 s while the old process exits, and removes the marker last so a halfway wipe repeats.
  - `unavailable_gateway.rs`: Graceful fallback gateway when sidecar is not installed so UI runs cleanly without crashing.
  - `service_wiring.rs`: Dependency injection constructor root (`build_services(&paths, &options) -> Result<UiServices>`). Degrades gracefully if sidecar or libmpv are missing.

#### `crates/ui_app`
- **Location**: `crates/ui_app/`
- **Dependencies**: `egui = "0.37"`, `eframe = "0.37"`, `egui_extras`, `image`, `player_api`, `app_services`
- **Architecture**: Unidirectional reactive pattern:
  - `screens/` and `widgets/` render state and emit `UiAction`.
  - `actions/dispatcher.rs` executes use cases asynchronously on `TaskRunner`.
  - Resulting `UiEvent` values update `AppModel`.
  - `event_handler.rs` applies changes and triggers UI redraws.
- **Navigation & Screens**:
  - Top Tabs (`navigation.rs`): `Home`, `Search`, `Library`, `Downloads`, `Settings`.
  - Screens: `home_screen.rs`, `search_screen.rs`, `details_screen.rs`, `library_screen.rs`, `downloads_screen.rs`, `settings_screen.rs`, `player_screen.rs`, `extensions_panel.rs`.
  - Glass/Backdrop Blur: Stretches player frame down to 48px and scales up via GPU linear filtering (`video_presenter::blur_thumbnail`, `widgets::glass`). Hero banners stretch artwork to 24x36px (`ambient_backdrop`).
- **Input State**: `UiServices` bundle passed to `run_desktop_app(services)`.

#### `crates/desktop_tauri`
- **Location**: `crates/desktop_tauri/`
- **Backend Architecture (`src/`)**:
  - `main.rs`: Registers IPC commands (including `get_resume_pointer`) and sets up `AppState`, `SourceManager`, and `StartupTracker`.
  - `source_manager.rs`: Session-based coordinator managing stream search, checking queue, title caching, and subtitle prefetch.
  - `source_fallback.rs` & `source_fallback_steps.rs`: Monitors playback and automatically fails over to the next best source upon playback error.
  - `check_gate.rs`: Manages wait limits: 30s cap for the first working source, 60s cap for complete search, or immediate playback on skip.
  - `library_commands.rs`: `get_bookmarks`, `add_bookmark`, `remove_bookmark`, `is_bookmarked`, `get_continue_watching`, `get_resume_pointer`, `remove_continue_watching`, `clear_continue_watching`. Purges title cache & subtitles via `SourceManager::forget_title`.
  - `video_commands.rs`: Delivers mpv RGBA frames to the frontend as binary data (`16-byte header: width, height, serial + pixels`).
  - `data_commands.rs`: `delete_all_data` (stops playback, starts the helper, closes the app).
  - `data_helper.rs`: the helper behind it: a PowerShell (Windows) or `sh` script written to the temp folder and started detached. It waits for the app's PID to exit, ends `java.exe` processes whose command line holds the data folder (the sidecar), deletes everything except `runtime-*` folders (retrying while files are in use), starts the app again (portable exe via `CS_PORTABLE_EXE`, AppImage via `APPIMAGE`, else the running program) and removes itself.
  - `update_commands.rs`: The self-updater (`check_for_updates`, `apply_update`). Reads the latest GitHub release and chooses the asset by name: `*-setup.exe` (installed Windows app), `*_portable.exe` (portable, replaced through `CS_PORTABLE_EXE`), `*.AppImage` whose name contains `x86_64` or `aarch64` (Linux). Keep the release file names (see `docs/RELEASING.md`).
- **Frontend Architecture (`frontend/src/`)**:
  - `App.tsx`: Main application shell, keyboard shortcuts, and tab routing (`home`, `browse`, `search`, `library`, `settings`).
  - `screens/`: `HomeScreen.tsx`, `BrowseScreen.tsx`, `SearchScreen.tsx`, `DetailsScreen.tsx` (shows "Resume" button with time left, highlights/scrolls to resume episode), `LibraryScreen.tsx`, `SettingsScreen.tsx`, `PlayerOverlay.tsx`.
  - `screens/settings/`: `GeneralSettings.tsx`, `PreferencesPage.tsx`, `SubtitleLanguages.tsx`, `RepositoryPage.tsx`, `PluginsTab.tsx`, `ExtensionsPage.tsx`.
  - `components/`: `VideoCanvas.tsx` (canvas pixel renderer), `SeekBar.tsx`, `SourceCheckPanel.tsx`, `OptionMenu.tsx`, `SubtitleFilter.tsx`, `PosterCard.tsx`, `BrowseCard.tsx`, `BrowseFilterBar.tsx`, `ContinueWatchingCard.tsx` (badge "S1 E3", progress bar), `ContextMenu.tsx`, `FeedbackPopup.tsx`, `DebugLoggingCard.tsx`, `StartupScreen.tsx`, `ResumeHero.tsx` (Home banner for the last unfinished title), `EpisodeButtons.tsx`, `UpdateIndicator.tsx` (nav bar update check) and `UpdateCard.tsx` (Settings > General update card), `NoPluginsPrompt.tsx` (Home's "add a plugin" card), `DeleteDataCard.tsx` (the "Delete all saved data" card at the bottom of Settings > General).
  - `components/details/`: `CastRow.tsx`, `EpisodeSection.tsx` (supports `highlightedKey` and auto-scrolling).
  - `playback/`: `episodeLookup.ts` (`orderedEpisodes`, `findNextEpisode`, `findPreviousEpisode`, `findEpisodeForKey`, `movieAsEpisode`, `playbackTitle`), `autoAdvance.ts` (`shouldAutoAdvance`: next episode starts when a video ended at >= 90 %), `skipPrompt.ts` (Skip Intro / Next episode button), `skipRequest.ts`.
  - `utils/episodeMarks.ts`: episode marks on the title page (bar while started, Watched from 80 %).
  - `utils/homeEmptyState.ts`: `homeEmptyState()` decides when Home shows `NoPluginsPrompt`: `no-plugins` on a fresh start (nothing installed), `plugins-unavailable` when plugins are installed but none gives a provider.
  - `utils/`: `continueWatching.ts` (`watchedPercent`, `episodeBadgeText`, `formatRemainingTime`, `continueKey`), `browseFilters.ts` (`GENRES`, `RATINGS`, `YEARS`, `defaultBrowseFilters`, `pickBestSearchMatch`).
  - `components/extensions/`: `IconTile.tsx`, `InstalledExtensionList.tsx`, `PluginList.tsx`, `RepositoryPanel.tsx`.
  - `hooks/`: `usePlayback.ts`, `usePlayerControls.ts`, `usePlayerPreferences.ts`, `usePlayerShortcuts.ts`, `useExtensions.ts`, `useAllPlugins.ts`, `useDebugLogging.ts`, `useStallWatch.ts`, `useStartup.ts`, `useWindowFullscreen.ts`.
  - `services/api.ts`: Typed TypeScript client invoking backend Tauri commands.

#### `crates/portable_launcher`
- **Location**: `crates/portable_launcher/`
- **Responsibility**: The single-file Windows portable exe. The release script compiles a tar.gz of the whole app into it (`CS_PAYLOAD_ARCHIVE`, see `build.rs`); on start it unpacks to `%LOCALAPPDATA%\CloudStreamRs\runtime-<version>\` once and launches `desktop_tauri.exe` from there. `build.rs` embeds `desktop_tauri/icons/icon.ico` as its icon.

---

### Chunk 8: Scripts, Tooling & External Repositories

#### Automation Scripts (`scripts/`)
- `scripts/setup_sidecar.ps1`:
  - Clones `CS3-desktop-client-unofficial` at commit `2d5a86ce2e7466494a9084776dcddd3626f633dd` to `C:\cs3d\repo`.
  - Overlays `sidecar/cs-sidecar` and builds the sidecar distribution with JDK 21.
- `scripts/run_tauri.ps1`:
  - Starts the Vite frontend server on port `5173`.
  - Sets `CS_DATA_DIR`, `CS_LIBMPV_PATH`, `CS_SIDECAR_HOME`, `CS_SIDECAR_JAVA_HOME`.
  - Runs `cargo run -p desktop_tauri`.
- `scripts/test_sidecar.ps1`:
  - Runs JUnit test suites for the Kotlin sidecar.
- `scripts/record_sidecar_replies.py`:
  - Executes the JVM sidecar with CineStream extension and saves sanitized golden JSON responses to `crates/extension_runtime/tests/fixtures/sidecar_replies/`.

- `scripts/build_release.ps1` (Windows PC):
  - Builds `CloudStream_<ver>_x64-setup.exe` (NSIS installer) and `CloudStream_<ver>_x64_portable.exe` into `dist/` (`-Kind installer|portable`).
  - Stages `libmpv-2.dll`, `vulkan-1.dll`, the sidecar (start script repaired for the `cmd.exe` line limit) and a Java 21 runtime; stops if `vulkan-1.dll` is missing, not x64, or absent from the installer resources or portable payload.
- `scripts/build_release.sh` (Linux PC): builds `CloudStream-<ver>-<x86_64|aarch64>.AppImage` for the PC's CPU (`--install-deps` installs the tools on Debian/Ubuntu).
- `scripts/vendor/vulkan/`: the vendored x64 Vulkan loader (`vulkan-1.dll`), its Apache-2.0 license and rebuild steps. libmpv imports it; PCs without a GPU driver have none.

#### GitHub Actions (`.github/workflows/`)
- `ci.yml`: format, clippy, Rust tests and frontend tests/build on every push and pull request to `main`.
- `release.yml`: builds the three release files on GitHub (Windows x64; AppImage on x86_64 and ARM runners) when a `v*` tag is pushed or when run by hand; a tag also drafts a release. See `docs/RELEASING.md`.

#### Sibling Reference Repositories
- `..\source code/`: Official CloudStream Android Kotlin codebase (`app/`, `library/`, `shared/`).
- `..\refrence code that can be copied from/codebase/`: Pinned `CS3-desktop-client-unofficial` codebase containing `android-stubs` and `plugin-runtime`.

---

## 3. Subsystem Lifecycles & Data Flow Diagrams

### Extension Execution & Crash Recovery

```mermaid
sequenceDiagram
    participant UI as Tauri Frontend / Service
    participant GW as RpcProviderGateway
    participant TR as SidecarProcessTransport
    participant JV as cs-sidecar (JVM)
    participant EX as Extension (.cs3)

    UI->>GW: search(provider, query, page)
    GW->>TR: call("search", json_params, timeout)
    TR->>JV: JSON-lines over stdin
    JV->>EX: provider.search()
    EX-->>JV: Kotlin SearchResponse
    JV-->>TR: JSON-lines over stdout
    TR-->>GW: raw serde_json::Value
    GW-->>UI: Result<SearchPage, GatewayError>

    Note over TR,JV: Crash Handling & Self-Healing
    JV--xTR: JVM Process Dies
    UI->>GW: next call
    GW->>TR: call(...)
    TR->>TR: Restart JVM Process
    TR->>JV: Replay remembered load_extension calls
    TR->>JV: Send original call
```

---

### Video Stream Resolution, Check Gate & Fallback

```mermaid
flowchart TD
    Start[User selects title / episode] --> CheckCache{TitleCache Lookup}
    
    CheckCache -- Fresh (<20 min) --> PlayCached[Play last good cached stream immediately]
    CheckCache -- Stale (20m - 24h) --> ProbeStale[Probe best 3 stale streams (3s cap)]
    CheckCache -- Miss --> FullSearch[Initiate streaming link search]
    
    ProbeStale -- One works --> PlayCached
    ProbeStale -- None work --> FullSearch
    
    PlayCached --> BgSearch[Fresh search runs in background]
    FullSearch --> Gate[CheckGate: 30s first working / 60s cap]
    Gate --> CheckQueue[CheckQueue: Priority sorted by resolution]
    CheckQueue --> StreamChecker[StreamChecker: HTTP HEAD / M3U8 check]
    
    Gate -- Stream verified OR user skips --> StartMpv[MpvPlayer::play]
    StartMpv --> Monitor[Playback Monitor]
    
    Monitor -- Stream error / sudden EOF --> Fallback[SourceFallback: Pick next verified stream]
    Fallback --> StartMpv
    
    BgSearch --> SaveCache[Save verified links to TitleCache on search completion]
```

---

### Subtitle Ingestion & Local Normalization

```mermaid
flowchart LR
    Provider[Provider Subtitle Track] --> Prefetch[SubtitlePrefetch: Filter by user language preference]
    Prefetch --> Fetcher[UreqSubtitleFetcher: Size cap & provider headers]
    Fetcher --> Normalize[normalize_subtitle: Unpack ZIP/GZ, convert UTF-16/CP1252 to UTF-8]
    Normalize --> Cache[SubtitleCache: Write to <data>/subtitles/<hash>.vtt]
    Cache --> Attach[MpvPlayer::add_subtitles: Local file attachment only]
```

---

## 4. Tauri IPC Interface (56 Commands Reference)

All 56 commands registered in `crates/desktop_tauri/src/main.rs`:

| Category | Command Identifier | Description |
|---|---|---|
| **Startup** | `get_startup_status` | Status of sidecar, libmpv, and extension loading. |
| **Preferences** | `get_preferences` | Audio/subtitle language preferences & auto-subtitle flag. |
| | `set_preferred_audio` | Sets preferred audio language. |
| | `set_preferred_subtitles` | Sets ordered subtitle language preference list. |
| | `set_auto_subtitles` | Enables or disables automatic subtitle track activation. |
| **Debug Logs** | `get_debug_logging` | Checks if debug logging is enabled. |
| | `set_debug_logging` | Toggles verbose debug logging. |
| | `open_logs_folder` | Opens OS file explorer at `<data_dir>/logs/`. |
| | `log_from_webapp` | Writes frontend console log messages into the backend trace. |
| **Catalog** | `get_available_providers` | Lists active providers and capabilities. |
| | `get_home_sections` | Fetches homepage shelves and banners. |
| | `search_media` | Queries multiple providers simultaneously. |
| | `get_media_details` | Loads metadata, cast, and episode list for a title. |
| | `browse_catalog` | Queries TMDB Discover API (60 items/page) filtered by media type, year, sort/rating, and genre. |
| **Playback** | `play_media` | Starts opening flow (cache check -> verification -> mpv). |
| | `skip_source_check` | Bypasses remaining stream checks and plays first ready stream. |
| | `pause_playback` | Pauses playback. |
| | `resume_playback` | Resumes playback. |
| | `seek_to` | Seeks to millisecond position. |
| | `stop_playback` | Stops player and clears active session. |
| | `get_playback_status` | Returns state, position, duration, and track list. |
| | `select_media_track` | Switches audio or subtitle track by ID. |
| | `set_volume` | Adjusts output volume percentage (0-130%). |
| **Sources** | `get_playback_sources` | Returns stream candidates and verification statuses. |
| | `switch_source` | Manually switches playback to another stream URL. |
| **Library** | `get_bookmarks` | Lists all bookmarked items. |
| | `add_bookmark` | Saves item to library. |
| | `remove_bookmark` | Removes item from library. |
| | `is_bookmarked` | Checks bookmark status. |
| | `get_continue_watching` | Returns in-progress watch history items. |
| | `remove_continue_watching`| Forgets progress and deletes cached sources for item. |
| | `clear_continue_watching` | Clears all continue-watching records. |
| **Settings** | `get_repositories` | Lists configured extension repositories. |
| | `add_repository` | Adds and probes a new extension repository. |
| | `remove_repository` | Removes an extension repository. |
| | `get_disabled_providers` | Lists provider IDs switched off by user. |
| | `set_provider_enabled` | Toggles provider enabled state. |
| **Extensions** | `get_installed_extensions` | Lists installed extensions and versions. |
| | `get_repository_plugins` | Lists plugins available in a repository. |
| | `refresh_repository_icons`| Background worker downloading repository icons. |
| | `install_plugin` | Downloads `.cs3` and registers extension. |
| | `uninstall_extension` | Deletes `.cs3` and removes providers. |
| | `set_extension_trusted` | Flags plugin as trusted (skipping sandboxing). |
| **Plugin Catalog**| `prepare_plugin_catalog` | Pre-indexes all repository plugins. |
| | `list_all_plugins` | Returns indexed plugins with repository metadata. |
| **Episodes** | `get_show_progress` | Saved position of every episode of a show (episode marks on the title page). |
| | `get_skip_segments` | Intro, recap and credits of the playing video from online databases. |
| | `set_playback_option` | Switches `autoplay_next` or `skip_lookup`. |
| | `provider_switched` | The viewer picked another provider: saved stream links from before are not used. |
| **Video/Window** | `set_video_surface` | Informs player of viewport dimensions. |
| | `get_video_frame` | Returns binary header + RGBA pixels for canvas rendering. |
| | `set_window_fullscreen` | Toggles fullscreen window state. |
| **Updates** | `check_for_updates` | Asks GitHub for the latest release and picks the file for this PC (installer, portable exe or AppImage, by file name). |
| | `apply_update` | Downloads that file, then a helper script swaps it in for the running program (the portable exe via `CS_PORTABLE_EXE`, the AppImage via `APPIMAGE`) and restarts it. |
| **Data** | `delete_all_data` | Settings > General > "Delete all saved data": starts a separate helper process (`data_helper.rs`) and closes the app; the helper ends leftover sidecar Java processes, deletes everything in the data folder (except `runtime-*`) and starts the app again. Fallback if the helper cannot start: a marker file that the next start acts on (`app_shell::data_wipe`). |

---

## 5. Code Navigation Guide ("Where is X?")

| Component / Task | File Location |
|---|---|
| **Tauri Backend Commands** | `crates/desktop_tauri/src/*_commands.rs` |
| **Tauri Backend Main & State** | `crates/desktop_tauri/src/main.rs`, `app_state.rs` |
| **Tauri Source Verification & Session**| `crates/desktop_tauri/src/source_manager.rs`, `source_session.rs` |
| **Tauri Stream Fallback Controller** | `crates/desktop_tauri/src/source_fallback.rs`, `source_fallback_steps.rs` |
| **Tauri Wait Limit Gate** | `crates/desktop_tauri/src/check_gate.rs` |
| **Tauri Video Frame Encoder** | `crates/desktop_tauri/src/video_commands.rs` |
| **Frontend Root & Router** | `crates/desktop_tauri/frontend/src/App.tsx` |
| **Frontend Canvas Blitter** | `crates/desktop_tauri/frontend/src/components/VideoCanvas.tsx` |
| **Frontend Tauri API Wrapper** | `crates/desktop_tauri/frontend/src/services/api.ts` |
| **Frontend TypeScript Types** | `crates/desktop_tauri/frontend/src/types/index.ts` |
| **Native GUI (egui)** | `crates/ui_app/src/screens/`, `crates/ui_app/src/widgets/` |
| **Service Construction Root** | `crates/app_shell/src/service_wiring.rs` |
| **OS Data Folder & Logs Setup** | `crates/app_shell/src/app_paths.rs`, `logging_setup.rs` |
| **Sidecar Process Transport** | `crates/extension_runtime/src/sidecar_process_transport.rs` |
| **Sidecar JSON-Lines RPC Gateway** | `crates/extension_runtime/src/rpc_provider_gateway.rs` |
| **libmpv FFI & Loaders** | `crates/player_mpv/src/mpv_api.rs`, `libmpv_locator.rs` |
| **SQLite Schema Migrations** | `crates/storage/src/schema_migrations.rs` |
| **Subtitle Cleaner & Decompressor** | `crates/subtitle_store/src/normalize_subtitle.rs` |
| **Stream Title Disk Cache** | `crates/title_cache/src/title_cache.rs` |
| **Delete all saved data** | `crates/app_shell/src/data_wipe.rs`, `crates/desktop_tauri/src/data_commands.rs`, `frontend/src/components/DeleteDataCard.tsx` |
| **Self-updater** | `crates/desktop_tauri/src/update_commands.rs` |
| **Home "add a plugin" prompt** | `frontend/src/components/NoPluginsPrompt.tsx`, `frontend/src/utils/homeEmptyState.ts` |
| **Release scripts & workflow** | `scripts/build_release.ps1`, `scripts/build_release.sh`, `.github/workflows/release.yml`, `docs/RELEASING.md` |
| **Portable exe launcher** | `crates/portable_launcher/` |
| **Workspace Coding Invariants** | `RULES.md` |

---

## 6. Step-by-Step Modification Recipes ("How do I do X?")

### Recipe 1: Adding a new Tauri IPC command (Rust + React)

1. **Create or update DTO**: In `crates/desktop_tauri/src/dto.rs`, declare request/response structs with `#[derive(Serialize, Deserialize)]`.
2. **Implement command function**: In the appropriate `crates/desktop_tauri/src/*_commands.rs`:
   ```rust
   #[tauri::command]
   pub async fn sample_command(input: String, state: State<'_, AppState>) -> Result<SampleDto, String> {
       let services = state.read();
       // Invoke services.catalog, services.library, etc.
       Ok(SampleDto { ... })
   }
   ```
3. **Register in Tauri handler**: In `crates/desktop_tauri/src/main.rs`, append `sample_commands::sample_command` to `tauri::generate_handler![...]`.
4. **Expose in frontend API**: In `crates/desktop_tauri/frontend/src/services/api.ts`:
   ```typescript
   export const api = {
     // ...
     sampleCommand: (input: string) => invokeTauri<SampleDto>('sample_command', { input }),
   };
   ```

---

### Recipe 2: Modifying or adding a React frontend component

1. **Location**: `crates/desktop_tauri/frontend/src/components/` or `screens/`.
2. **Tailwind CSS**: Use existing theme colors (`bg-[#0c0f17]`, `border-white/10`, `text-white/80`).
3. **State Management**: Consume hooks from `frontend/src/hooks/` or invoke backend commands via `api` from `services/api.ts`.
4. **Build Check**: In `crates/desktop_tauri/frontend`, run `npm run build` to verify TypeScript typing and JSX bundling.

---

### Recipe 3: Adding or updating an Application Service

1. **Location**: `crates/app_services/src/`.
2. **Design Limits**:
   - Files ≤ 300 lines. Functions ≤ 40 lines, ≤ 4 arguments.
   - Accept collaborators via traits (`Arc<dyn ProviderGateway>`, stores, `Arc<dyn Clock>`).
   - Log significant lifecycle steps with `tracing::info!` and warnings with `tracing::warn!`.
3. **Export**: Declare module and re-export in `crates/app_services/src/lib.rs`.
4. **Wire in Composition Root**: Update `build_services` in `crates/app_shell/src/service_wiring.rs` to construct the service.

---

### Recipe 4: Adding a database migration and store method

1. **Append Migration**: In `crates/storage/src/schema_migrations.rs`, append a new SQL string to `MIGRATIONS`:
   ```rust
   const MIGRATIONS: &[&str] = &[
       // ... existing migrations
       "ALTER TABLE bookmarks ADD COLUMN custom_tag TEXT;",
   ];
   ```
   *Never modify or reorder existing migrations.*
2. **Implement Store Query**: In the target store (e.g. `bookmark_store.rs`), implement methods using rusqlite named parameter bindings (`params![...]`).
3. **Write Unit Test**: Use `Database::open_in_memory()` to verify migration execution and query behavior.

---

### Recipe 5: Modifying stream checking or playback fallback

1. **Stream Checking**: In `crates/app_services/src/stream_checker.rs`. Probes HEAD/GET status codes and playlist availability.
2. **Wait Logic**: In `crates/desktop_tauri/src/check_gate.rs`. Configures `WaitLimits` (30s first working link timeout, 60s total search timeout).
3. **Fallback Logic**: In `crates/desktop_tauri/src/source_fallback.rs` and `source_fallback_steps.rs`. When playback reports `PlaybackState::Failed`, `FallbackController::on_status` invokes `fail_over`, uses `next_candidate(&active)` from `ActiveSources` (`source_session.rs`) to select the next working/unchecked stream, marks the failed stream broken via `sources.mark_broken`, starts the candidate at the previous position via `start_source`, and tracks limits via `FallbackLimits` (max 3 attempts or 20s cap). When a fallback stream plays successfully, `on_playing` records it via `sources.note_played`.

---

### Recipe 6: Writing concurrent code and mandatory race tests

> [!IMPORTANT]
> Any file introducing or modifying shared state across threads (`Mutex`, `RwLock`, atomics, channels, worker threads) **must include race tests** in the same change.

Required race test elements:
1. **Multi-threaded Stress Testing**: Spawn multiple threads running hundreds or thousands of iterations while asserting state invariants hold.
2. **Deterministic Interleaving**: Step through known race edge cases (e.g. source switch arriving precisely when stream probe finishes).
3. **Proof of Failure**: Verify that removing synchronization or generation guards causes the test to fail.
4. **Generation Counters**: Protect asynchronous requests with a generation `u64` to discard stale thread completions.

---

## 7. Invariants, Quality Gates & Project Rules (RULES.md)

All code must pass these quality gates before commit:

| Check | Tool / Rule | Constraint |
|---|---|---|
| **Formatting** | `cargo fmt --all -- --check` | Standard Rust formatting (`rustfmt.toml`). |
| **Lints** | `cargo clippy --workspace --all-targets -- -D warnings` | Zero warnings permitted across workspace. |
| **File Limit** | Clippy `too_many_lines` | ≤ 300 lines per file. |
| **Function Limit**| Clippy `too_many_lines` | ≤ 40 lines per function. |
| **Parameter Limit**| Clippy `too_many_arguments` | ≤ 4 parameters per function (use structs for more). |
| **Complexity** | Clippy `cognitive_complexity` | ≤ 10 per function. |
| **Panics / Unwraps**| Clippy restrictions | Deny `unwrap_used`, `expect_used`, `panic`, `unreachable`, `todo`. |
| **Indexing** | Clippy `indexing_slicing` | Denied. Must use `.get()` or pattern matching. |
| **Unsafe Code** | `#![forbid(unsafe_code)]` | Forbidden everywhere except `player_mpv/src/mpv_api.rs`. |
| **Token Safety** | `title_cache::redact_url` | Never log sensitive tokens or complete signed stream links. |
| **License Check** | `cargo deny check` | Validates GPL-3.0 compatibility and dependency advisories. |

---

## 8. Developer Commands & Environment Variables

### Daily Developer Commands
```powershell
# Run complete verification gates (fmt, clippy, tests, deny, audit)
just gates

# Run all unit and integration tests across workspace
cargo test --workspace

# Run Tauri desktop app in dev mode (spins up Vite + Tauri)
powershell -File scripts/run_tauri.ps1

# Run native egui desktop shell
cargo run -p app_shell

# Build the Kotlin JVM extension sidecar
just sidecar

# Run Kotlin sidecar JUnit tests
powershell -File scripts/test_sidecar.ps1

# Windows x64 release files into dist/: NSIS installer + single-file portable exe (-Kind installer|portable)
powershell -File scripts/build_release.ps1

# Linux AppImage for this PC's CPU; first run: scripts/build_release.sh --install-deps
scripts/build_release.sh

# GitHub builds all of them: .github/workflows/release.yml (manual run, or a v* tag makes a draft release)
```

### Environment Variables
- `CS_DATA_DIR`: Base path for data storage (defaults to `%LOCALAPPDATA%\CloudStreamRs` on Windows, `~/.local/share/cloudstream-rs` on Linux).
- `CS_SIDECAR_HOME`: Directory containing `cs-sidecar` distribution (`bin/cs-sidecar.bat`).
- `CS_SIDECAR_JAVA_HOME`: JDK 21 installation path for the sidecar.
- `CS_LIBMPV_PATH`: Explicit file path to `libmpv-2.dll` / `libmpv.so`.
- `CS_MPV_LOG`: When set, enables verbose internal libmpv console logging (`terminal=yes, msg-level=all=v`).
- `CS_TEST_CINESTREAM_CS3`: Path to CineStream `.cs3` extension file used by `scripts/record_sidecar_replies.py`.
- Packages (`scripts/build_release.ps1`) carry `libmpv-2.dll`, `sidecar\` and `jre\` next to the program; the app finds them there (`sidecar_discovery::bundled_install`). A `portable.txt` next to the program makes it keep its data in `data\` beside it (`app_paths::portable_data_dir`).
- `RUST_LOG`: Tracing filter directive (e.g. `RUST_LOG=cloudstream=debug,desktop_tauri=debug`).
