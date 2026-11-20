# extension_api

The contract between the app and an extension runtime. Implementations (JVM sidecar pool,
fakes, unavailable stub) live elsewhere; app services and shells depend only on this crate.

- **Public API:** `ProviderGateway`, `GatewayError`.
- **`ProviderGateway`:** load/unload extensions; `search`, `main_page`, `load`, `load_links`,
  streaming link search (`start_link_search` / `poll_link_search` / `cancel_link_search`),
  `extract`, `take_added_repositories`, `extend_call_deadline`, `shutdown`, `set_debug_logging`,
  resident-worker cap (`set_resident_worker_limit`). Optional methods have no-op or `Unavailable`
  defaults for runtimes that cannot support them.
- **`GatewayError`:** `Unavailable`, `Timeout`, `Transport`, `Remote`, `Decode`.
- **Test:** `cargo test -p extension_api`
- **Rules:** follow the workspace `RULES.md`.
