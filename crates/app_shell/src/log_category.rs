//! Which debug log file a message belongs in, judged by the module that wrote it.

/// File that collects every warning and error of every category.
pub(crate) const PROBLEMS_FILE: &str = "warnings-and-errors.jsonl";

/// Our own crates and the pseudo target of the sidecar; their debug/trace output is kept while
/// debug logging is on. Other crates only show warnings and errors.
pub(crate) const OWN_TARGETS: &[&str] = &[
    "app_services",
    "app_shell",
    "app_updater",
    "cloudflare_bridge",
    "cloudflare_window",
    "cloudstream",
    "core_model",
    "extension_api",
    "extension_repo",
    "extension_runtime",
    "player_api",
    "player_mpv",
    "playback_session",
    "poster_artwork",
    "sidecar",
    "skip_times",
    "storage",
    "stream_proxy",
    "subtitle_store",
    "title_cache",
    "ui_slint",
];

/// A part of the app with its own structured JSON Lines debug log file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LogCategory {
    /// Start-up, catalog/browse/library/settings services, updates, artwork, skips.
    App,
    /// Slint desktop user interface: screens, navigation, focus, and user actions.
    Ui,
    /// The video player: mpv, playback state, tracks, frames, subtitle downloads.
    Player,
    /// The JVM sidecar and the plugins running in it.
    Sidecar,
    /// Loading, installing and calling extensions and repositories.
    Extensions,
    /// Finding, checking and switching streams, stream proxy, and saved stream links.
    Streams,
    /// Cloudflare verification: the sidecar's challenge handling and the verification window.
    Cloudflare,
    /// The database, storage usage, scheduled/manual cleanup, and data wipe.
    Storage,
}

/// Module path prefixes and the category they belong to; the first match wins.
const ROUTES: &[(&str, LogCategory)] = &[
    ("sidecar", LogCategory::Sidecar),
    ("extension_runtime::cloudflare", LogCategory::Cloudflare),
    ("cloudflare_", LogCategory::Cloudflare),
    ("ui_slint::bridge::cloudflare", LogCategory::Cloudflare),
    ("player_", LogCategory::Player),
    ("app_services::playback", LogCategory::Player),
    ("subtitle_store", LogCategory::Player),
    ("ui_slint::bridge::player", LogCategory::Player),
    ("ui_slint::bridge::subtitle", LogCategory::Player),
    ("ui_slint::playback_watch", LogCategory::Player),
    ("extension_", LogCategory::Extensions),
    ("app_services::extension", LogCategory::Extensions),
    ("app_services::repository", LogCategory::Extensions),
    ("app_services::plugin_index", LogCategory::Extensions),
    ("app_services::stream_resolver", LogCategory::Extensions),
    ("ui_slint::bridge::extension", LogCategory::Extensions),
    ("ui_slint::repository_discovery", LogCategory::Extensions),
    ("app_services::stream_checker", LogCategory::Streams),
    ("app_services::link_search", LogCategory::Streams),
    ("app_services::stream_label", LogCategory::Streams),
    ("stream_proxy", LogCategory::Streams),
    ("title_cache", LogCategory::Streams),
    ("playback_session", LogCategory::Streams),
    ("ui_slint::bridge::stream", LogCategory::Streams),
    ("ui_slint::bridge::source", LogCategory::Streams),
    ("storage", LogCategory::Storage),
    ("app_services::storage_", LogCategory::Storage),
    ("app_services::regenerable_cleanup", LogCategory::Storage),
    ("app_shell::cleanup_wiring", LogCategory::Storage),
    ("app_shell::data_wipe", LogCategory::Storage),
    ("app_shell::wipe_helper", LogCategory::Storage),
    ("ui_slint::bridge::storage", LogCategory::Storage),
    ("ui_slint", LogCategory::Ui),
];

impl LogCategory {
    /// Every category, in the order they are listed to the user.
    pub(crate) const ALL: [LogCategory; 8] = [
        LogCategory::App,
        LogCategory::Ui,
        LogCategory::Player,
        LogCategory::Sidecar,
        LogCategory::Extensions,
        LogCategory::Streams,
        LogCategory::Cloudflare,
        LogCategory::Storage,
    ];

    /// The category of messages written by `target` (a module path).
    pub(crate) fn for_target(target: &str) -> Self {
        ROUTES
            .iter()
            .find(|(prefix, _)| target.starts_with(prefix))
            .map_or(LogCategory::App, |(_, category)| *category)
    }

    /// Name of the JSON Lines file this category is written to.
    pub(crate) fn file_name(self) -> &'static str {
        match self {
            LogCategory::App => "app-logs.jsonl",
            LogCategory::Ui => "ui-logs.jsonl",
            LogCategory::Player => "player-logs.jsonl",
            LogCategory::Sidecar => "sidecar-logs.jsonl",
            LogCategory::Extensions => "extensions-logs.jsonl",
            LogCategory::Streams => "streams-logs.jsonl",
            LogCategory::Cloudflare => "cloudflare-logs.jsonl",
            LogCategory::Storage => "storage-logs.jsonl",
        }
    }

    /// What the category covers, for diagnostics displays.
    pub(crate) fn label(self) -> &'static str {
        match self {
            LogCategory::App => "App, catalog and updates",
            LogCategory::Ui => "User interface",
            LogCategory::Player => "Video player and subtitles",
            LogCategory::Sidecar => "JVM sidecar and plugins",
            LogCategory::Extensions => "Extensions and repositories",
            LogCategory::Streams => "Stream finding, checking and proxy",
            LogCategory::Cloudflare => "Cloudflare verification",
            LogCategory::Storage => "Database and storage cleanup",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_verification_has_its_own_file_on_all_sides() {
        assert_eq!(
            LogCategory::for_target("cloudflare_window::site_window"),
            LogCategory::Cloudflare
        );
        assert_eq!(
            LogCategory::for_target("cloudflare_bridge::verify_helper"),
            LogCategory::Cloudflare
        );
        assert_eq!(
            LogCategory::for_target("ui_slint::bridge::cloudflare_callbacks"),
            LogCategory::Cloudflare
        );
        assert_eq!(
            LogCategory::for_target("extension_runtime::cloudflare_sidecar"),
            LogCategory::Cloudflare
        );
        assert_eq!(LogCategory::Cloudflare.file_name(), "cloudflare-logs.jsonl");
    }

    #[test]
    fn saved_cloudflare_passes_log_to_the_cloudflare_file() {
        for module in [
            "cloudflare_bridge::watch_passes",
            "cloudflare_bridge::disk_pass_book",
            "cloudflare_bridge::verify_helper",
            "cloudflare_window::browser_cookies",
        ] {
            assert_eq!(
                LogCategory::for_target(module),
                LogCategory::Cloudflare,
                "{module}"
            );
        }
        assert!(OWN_TARGETS.contains(&"cloudflare_bridge"));
        assert!(OWN_TARGETS.contains(&"cloudflare_window"));
    }

    #[test]
    fn modules_are_routed_to_their_own_jsonl_file() {
        assert_eq!(LogCategory::for_target("sidecar"), LogCategory::Sidecar);
        assert_eq!(
            LogCategory::for_target("ui_slint::bridge::catalog_callbacks"),
            LogCategory::Ui
        );
        assert_eq!(
            LogCategory::for_target("ui_slint::bridge::player_callbacks"),
            LogCategory::Player
        );
        assert_eq!(
            LogCategory::for_target("player_mpv::mpv_log"),
            LogCategory::Player
        );
        assert_eq!(
            LogCategory::for_target("app_services::stream_checker"),
            LogCategory::Streams
        );
        assert_eq!(
            LogCategory::for_target("ui_slint::bridge::stream_opening"),
            LogCategory::Streams
        );
        assert_eq!(
            LogCategory::for_target("app_services::storage_cleanup::runner"),
            LogCategory::Storage
        );
        assert_eq!(
            LogCategory::for_target("storage::database"),
            LogCategory::Storage
        );
    }

    #[test]
    fn the_stream_proxy_and_saved_links_and_subtitles_route_cleanly() {
        assert_eq!(
            LogCategory::for_target("stream_proxy::request_handler"),
            LogCategory::Streams
        );
        assert_eq!(
            LogCategory::for_target("title_cache::title_cache"),
            LogCategory::Streams
        );
        assert_eq!(
            LogCategory::for_target("subtitle_store::subtitle_loader"),
            LogCategory::Player
        );
        assert_eq!(
            LogCategory::for_target("extension_runtime::sidecar_process_transport"),
            LogCategory::Extensions
        );
        assert_eq!(
            LogCategory::for_target("wgpu_hal::vulkan"),
            LogCategory::App
        );
    }

    #[test]
    fn every_category_has_a_distinct_jsonl_file() {
        let mut names: Vec<&str> = LogCategory::ALL.iter().map(|c| c.file_name()).collect();
        assert!(names.iter().all(|name| name.ends_with(".jsonl")));
        assert!(PROBLEMS_FILE.ends_with(".jsonl"));
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LogCategory::ALL.len());
        assert!(!names.contains(&PROBLEMS_FILE));
    }

    /// Workspace crates that write no logs: the portable launcher only unpacks and starts the app,
    /// and `memory_budget` and `content_policy` are pure (no I/O, no logging).
    const CRATES_WITHOUT_LOGS: &[&str] = &["portable_launcher", "memory_budget", "content_policy"];

    #[test]
    fn own_targets_every_workspace_crate_that_logs_is_listed() {
        let workspace = include_str!("../../../Cargo.toml");
        let missing: Vec<&str> = workspace
            .lines()
            .filter_map(|line| line.trim().strip_prefix("\"crates/"))
            .filter_map(|rest| rest.strip_suffix("\","))
            .filter(|name| !CRATES_WITHOUT_LOGS.contains(name) && !OWN_TARGETS.contains(name))
            .collect();
        assert!(missing.is_empty(), "debug output of {missing:?} is dropped");
    }
}
