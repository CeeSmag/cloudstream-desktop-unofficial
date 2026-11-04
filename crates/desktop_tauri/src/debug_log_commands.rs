//! Tauri commands for the debug logging switch of the General settings page.

use app_services::SettingsService;
use app_shell::{DebugLogs, LogFileInfo};
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;

/// A debug log file, for the settings page.
#[derive(Debug, Clone, Serialize)]
pub struct LogFileDto {
    /// What the file covers.
    pub label: String,
    /// File name inside the folder.
    pub file_name: String,
    /// Size in bytes; absent while the file does not exist.
    pub size_bytes: Option<u64>,
}

/// State of debug logging.
#[derive(Debug, Clone, Serialize)]
pub struct DebugLoggingDto {
    /// Whether debug logs are being written.
    pub enabled: bool,
    /// Folder the files are in.
    pub folder: String,
    /// The files, problems file last.
    pub files: Vec<LogFileDto>,
}

/// Applies the choice saved last time, so debug logging stays on across restarts. Returns
/// whether it is on, so the extension runtime can be told when it starts.
pub fn restore_saved_choice(settings: &SettingsService, debug_logs: &DebugLogs) -> bool {
    match settings.debug_logging_enabled() {
        Ok(true) => match debug_logs.set_enabled(true) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "cannot restore debug logging");
                false
            }
        },
        Ok(false) => false,
        Err(error) => {
            tracing::warn!(%error, "cannot read the debug logging setting");
            false
        }
    }
}

/// Whether debug logging is on, where the files are, and how big they are.
#[tauri::command]
pub async fn get_debug_logging(
    debug_logs: State<'_, DebugLogs>,
) -> Result<DebugLoggingDto, String> {
    Ok(describe(&debug_logs))
}

/// Switches debug logging on or off and remembers the choice.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(enabled = ?enabled), err)]
pub async fn set_debug_logging(
    enabled: bool,
    state: State<'_, AppState>,
    debug_logs: State<'_, DebugLogs>,
) -> Result<DebugLoggingDto, String> {
    state
        .read()
        .settings
        .set_debug_logging_enabled(enabled)
        .map_err(|error| error.to_string())?;
    debug_logs
        .set_enabled(enabled)
        .map_err(|error| format!("Cannot write the debug logs: {error}"))?;
    state.read().extensions.set_debug_logging(enabled);
    // The player's own (mpv) messages are part of the debug logs, at a level that needs no restart.
    if let Some(playback) = &state.read().playback {
        playback.set_verbose_player_log(enabled);
    }
    Ok(describe(&debug_logs))
}

/// Opens the debug log folder in the file explorer.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn open_logs_folder(debug_logs: State<'_, DebugLogs>) -> Result<(), String> {
    let folder = debug_logs.directory();
    std::fs::create_dir_all(folder)
        .map_err(|error| format!("Cannot create {}: {error}", folder.display()))?;
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(folder)
        .spawn()
        .map(drop)
        .map_err(|error| format!("Cannot open {}: {error}", folder.display()))
}

/// Writes a message of the web app to the logs; its category is `webapp`.
#[tauri::command]
pub async fn log_from_webapp(level: String, message: String) -> Result<(), String> {
    write_webapp_log(&level, &message)
}

fn write_webapp_log(level: &str, message: &str) -> Result<(), String> {
    match level {
        "error" => tracing::error!(target: "webapp", "{message}"),
        "warn" => tracing::warn!(target: "webapp", "{message}"),
        "info" => tracing::info!(target: "webapp", "{message}"),
        "debug" => tracing::debug!(target: "webapp", "{message}"),
        "trace" => tracing::trace!(target: "webapp", "{message}"),
        other => return Err(format!("Unknown log level: {other}")),
    }
    Ok(())
}

fn describe(debug_logs: &DebugLogs) -> DebugLoggingDto {
    DebugLoggingDto {
        enabled: debug_logs.is_enabled(),
        folder: debug_logs.directory().display().to_string(),
        files: debug_logs.files().iter().map(to_dto).collect(),
    }
}

fn to_dto(file: &LogFileInfo) -> LogFileDto {
    LogFileDto {
        label: file.label.to_owned(),
        file_name: file.file_name.to_owned(),
        size_bytes: file.size_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_web_app_levels_are_refused() {
        assert!(write_webapp_log("loud", "x").is_err());
        assert!(write_webapp_log("info", "x").is_ok());
    }

    #[test]
    fn the_description_lists_every_file_and_starts_off() {
        let logs = DebugLogs::new(std::env::temp_dir().join("cs_debug_dto"));
        let described = describe(&logs);
        assert!(!described.enabled);
        assert_eq!(described.files.len(), 8);
        assert!(described.files.iter().all(|file| file.size_bytes.is_none()));
    }
}
