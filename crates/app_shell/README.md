# app_shell

The `cloudstream` product binary. It finds the data folder, optionally wipes it, starts
logging, builds services (`service_wiring.rs`), starts the Cloudflare watcher, and hands
`UiServices` plus `AppExtras` to `ui_slint` (`run_desktop_app_with`).

Startup order in `main.rs`: `wipe_if_requested` → logging → `build_services` → stale update
cleanup thread → `start_cloudflare_watch` → Slint UI. "Delete all saved data" only marks the
folder; the next start clears it before anything else opens.

`build_services` wires SQLite stores, a `PluginWorkerPool` gateway (or `UnavailableGateway`),
extension/catalog/library/settings services, `TitleCache` / `SubtitleCache`, poster
`ArtworkLoader`, `SkipLookup`, and a `RoutedPlayer` around libmpv when present. Existing
installs keep an unlimited plugin-worker cap until the setting is changed.

Missing pieces degrade instead of crashing:

| Missing | Result |
|---|---|
| Sidecar (`CS_SIDECAR_HOME` / bundle) | App starts; extension calls report "runtime not available" |
| libmpv (`CS_LIBMPV_PATH` / auto-detect) | App starts; Play shows why playback is unavailable |
| Verify helper (`CS_VERIFY_HELPER` / next to exe) | App starts; Cloudflare sites cannot be verified |

The stream proxy (`stream_proxy_router.rs`) wraps the player in `service_wiring.rs`. While
Settings allows it, HLS (and progressive/unknown streams with custom headers or a referer)
go through the local helper; DASH never does. The proxy starts on the first such stream; if
it cannot open, the stream plays directly.

Environment: `CS_DATA_DIR`, `CS_SIDECAR_HOME`, `CS_SIDECAR_JAVA_HOME`, `CS_LIBMPV_PATH`,
`CS_VERIFY_HELPER`, `RUST_LOG`. A `portable.txt` next to the program keeps data beside it.
