//! Which debug log file a message belongs in, judged by the module that wrote it.

/// File that collects every warning and error of every category.
pub const PROBLEMS_FILE: &str = "warnings-and-errors.txt";

/// Our own crates and the pseudo targets of the sidecar and the web app; their debug output is
/// kept while debug logging is on. Other crates only show warnings and errors.
pub const OWN_TARGETS: &[&str] = &[
    "app_services",
    "app_shell",
    "cloudstream",
    "core_model",
    "desktop_tauri",
    "extension_api",
    "extension_repo",
    "extension_runtime",
    "extractors",
    "html_parsing",
    "network_client",
    "player_api",
    "player_mpv",
    "sidecar",
    "storage",
    "ui_app",
    "webapp",
];

/// A part of the app with its own debug log file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogCategory {
    /// Start-up, commands, settings and everything not listed below.
    App,
    /// The video player: mpv, playback state, tracks, frames.
    Player,
    /// The JVM sidecar and the plugins running in it.
    Sidecar,
    /// Loading, installing and calling extensions and repositories.
    Extensions,
    /// Finding, checking and switching streams.
    Streams,
    /// Cloudflare verification: the sidecar's challenge handling and the verification window.
    Cloudflare,
    /// The database.
    Storage,
    /// The user interface (the web app).
    Webapp,
}

/// Module path prefixes and the category they belong to; the first match wins.
const ROUTES: &[(&str, LogCategory)] = &[
    ("sidecar", LogCategory::Sidecar),
    ("desktop_tauri::cloudflare", LogCategory::Cloudflare),
    ("extension_runtime::cloudflare", LogCategory::Cloudflare),
    ("webapp", LogCategory::Webapp),
    ("player_", LogCategory::Player),
    ("app_services::playback", LogCategory::Player),
    ("desktop_tauri::playback_commands", LogCategory::Player),
    ("desktop_tauri::video_commands", LogCategory::Player),
    ("extension_", LogCategory::Extensions),
    ("app_services::extension", LogCategory::Extensions),
    ("app_services::repository", LogCategory::Extensions),
    ("app_services::stream_resolver", LogCategory::Extensions),
    ("desktop_tauri::extension_commands", LogCategory::Extensions),
    (
        "desktop_tauri::repository_discovery",
        LogCategory::Extensions,
    ),
    ("desktop_tauri::startup", LogCategory::Extensions),
    ("app_services::stream_checker", LogCategory::Streams),
    ("desktop_tauri::source", LogCategory::Streams),
    ("desktop_tauri::check_gate", LogCategory::Streams),
    ("storage", LogCategory::Storage),
];

impl LogCategory {
    /// Every category, in the order they are listed to the user.
    pub const ALL: [LogCategory; 8] = [
        LogCategory::App,
        LogCategory::Player,
        LogCategory::Sidecar,
        LogCategory::Extensions,
        LogCategory::Streams,
        LogCategory::Cloudflare,
        LogCategory::Storage,
        LogCategory::Webapp,
    ];

    /// The category of messages written by `target` (a module path).
    pub fn for_target(target: &str) -> Self {
        ROUTES
            .iter()
            .find(|(prefix, _)| target.starts_with(prefix))
            .map_or(LogCategory::App, |(_, category)| *category)
    }

    /// Name of the text file this category is written to.
    pub fn file_name(self) -> &'static str {
        match self {
            LogCategory::App => "app-logs.txt",
            LogCategory::Player => "player-logs.txt",
            LogCategory::Sidecar => "sidecar-logs.txt",
            LogCategory::Extensions => "extensions-logs.txt",
            LogCategory::Streams => "streams-logs.txt",
            LogCategory::Cloudflare => "cloudflare-logs.txt",
            LogCategory::Storage => "storage-logs.txt",
            LogCategory::Webapp => "webapp-logs.txt",
        }
    }

    /// What the category covers, for the settings page.
    pub fn label(self) -> &'static str {
        match self {
            LogCategory::App => "App and commands",
            LogCategory::Player => "Video player",
            LogCategory::Sidecar => "JVM sidecar and plugins",
            LogCategory::Extensions => "Extensions and repositories",
            LogCategory::Streams => "Stream finding and checking",
            LogCategory::Cloudflare => "Cloudflare verification",
            LogCategory::Storage => "Database",
            LogCategory::Webapp => "User interface",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_verification_has_its_own_file_on_both_sides() {
        let app = LogCategory::for_target("desktop_tauri::cloudflare_window");
        let sidecar = LogCategory::for_target("extension_runtime::cloudflare_sidecar");
        assert_eq!(app, LogCategory::Cloudflare);
        assert_eq!(sidecar, LogCategory::Cloudflare);
        assert_eq!(LogCategory::Cloudflare.file_name(), "cloudflare-logs.txt");
    }

    #[test]
    fn modules_are_routed_to_their_own_file() {
        assert_eq!(LogCategory::for_target("sidecar"), LogCategory::Sidecar);
        assert_eq!(LogCategory::for_target("webapp"), LogCategory::Webapp);
        assert_eq!(
            LogCategory::for_target("player_mpv::event_worker"),
            LogCategory::Player
        );
        assert_eq!(
            LogCategory::for_target("desktop_tauri::playback_commands"),
            LogCategory::Player
        );
        assert_eq!(
            LogCategory::for_target("app_services::stream_checker"),
            LogCategory::Streams
        );
        assert_eq!(
            LogCategory::for_target("storage::database"),
            LogCategory::Storage
        );
    }

    #[test]
    fn the_sidecar_transport_belongs_to_extensions_not_the_sidecar_file() {
        assert_eq!(
            LogCategory::for_target("extension_runtime::sidecar_process_transport"),
            LogCategory::Extensions
        );
    }

    #[test]
    fn unknown_modules_land_in_the_app_file() {
        assert_eq!(
            LogCategory::for_target("wgpu_hal::vulkan"),
            LogCategory::App
        );
        assert_eq!(LogCategory::for_target("desktop_tauri"), LogCategory::App);
    }

    #[test]
    fn every_category_has_a_distinct_file() {
        let mut names: Vec<&str> = LogCategory::ALL.iter().map(|c| c.file_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LogCategory::ALL.len());
        assert!(!names.contains(&PROBLEMS_FILE));
    }
}
