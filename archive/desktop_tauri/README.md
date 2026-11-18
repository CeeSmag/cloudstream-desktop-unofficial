# Archived: `desktop_tauri` (WebView product)

This folder is the former Tauri + React shell. It is **not** a Cargo workspace member in
`cloudstream-rs-egui`. Product builds ship `cloudstream` from `crates/app_shell` (egui).

Kept as a reference while SourceManager / Cloudflare / updater behaviour is ported into
`playback_session`, `cloudflare_bridge`, and `ui_app`. Do not add it back to `[workspace].members`
unless you intentionally revive the WebView product.
