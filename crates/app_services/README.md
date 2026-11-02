# app_services

The use cases behind the screens. Each service takes its collaborators as trait objects or stores,
so it is tested with fakes and wired to the real thing only in `app_shell`.

| Service | Responsibility |
|---|---|
| `RepositoryService` | add/remove repositories, list their plugins |
| `ExtensionService` | install/uninstall/load plugin files into the runtime |
| `CatalogService` | search, home page, title details |
| `StreamResolver` | turn a title/episode into playable streams, with retry and fallback |
| `PlaybackService` | start playback, resume position, save progress |
| `LibraryService` | bookmarks and continue-watching |
| `SettingsService` | typed settings |

Test: `cargo test -p app_services`.
