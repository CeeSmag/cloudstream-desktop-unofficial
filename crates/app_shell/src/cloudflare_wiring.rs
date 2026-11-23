//! Starts the Cloudflare verification watcher and finds its helper program.

use std::path::{Path, PathBuf};

use app_services::SettingsService;
use cloudflare_bridge::{
    spawn_watcher, AllowedHosts, HelperLaunch, SavedPasses, WatchConfig, WatchHandle,
};

/// Overrides where the verification helper is (for development builds).
const HELPER_VARIABLE: &str = "CS_VERIFY_HELPER";

/// The helper's file name next to the app's program.
fn helper_file_name() -> &'static str {
    if cfg!(windows) {
        "cloudstream-verify.exe"
    } else {
        "cloudstream-verify"
    }
}

/// Starts watching `<data>/sidecar/cloudflare`, with the sites the viewer allowed before.
pub fn start_cloudflare_watch(data_dir: &Path, settings: &SettingsService) -> WatchHandle {
    let exchange = data_dir.join("sidecar").join("cloudflare");
    cloudflare_bridge::clear_folder(&exchange);
    let saved = settings.cloudflare_allowed_hosts().unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read the allowed Cloudflare sites; asking again");
        Vec::new()
    });
    let helper = find_helper().map(|program| HelperLaunch {
        program,
        args: helper_args(&exchange, &data_dir.join("cloudflare_browser")),
    });
    if helper.is_none() {
        tracing::warn!(
            "no Cloudflare verification helper next to the program; such sites cannot be verified"
        );
    }
    let watch = spawn_watcher(WatchConfig {
        dir: exchange,
        allowed: AllowedHosts::new(saved),
        helper,
        passes: saved_passes(data_dir),
    });
    let idle = settings
        .helper_idle()
        .map(memory_budget::HelperIdle::as_duration)
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "verification helper idle time not read; keeping it open");
            None
        });
    watch.set_helper_idle(idle);
    match settings.helper_windows() {
        Ok(windows) => watch.set_helper_windows(windows.count()),
        Err(error) => {
            tracing::warn!(%error, "verification window limit not read; the helper uses its own");
        }
    }
    watch
}

/// Passes kept in `<data>/cloudflare_passes`, outside `<data>/sidecar` (the only part of the data folder a
/// plugin's sandbox may read). Without the folder every request is checked in the window, as before.
fn saved_passes(data_dir: &Path) -> Option<SavedPasses> {
    SavedPasses::on_disk(passes_dir(data_dir))
        .map_err(|error| {
            tracing::warn!(%error, "saved Cloudflare passes are off: every site is checked again");
        })
        .ok()
}

/// Where saved passes live: beside the sidecar's folder, never inside it.
fn passes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("cloudflare_passes")
}

fn helper_args(exchange: &Path, profile: &Path) -> Vec<String> {
    vec![
        "--exchange".to_owned(),
        exchange.to_string_lossy().into_owned(),
        "--profile".to_owned(),
        profile.to_string_lossy().into_owned(),
    ]
}

/// `CS_VERIFY_HELPER`, else `cloudstream-verify` next to the running program.
fn find_helper() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(HELPER_VARIABLE).map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let program = std::env::current_exe().ok()?;
    let helper = program.parent()?.join(helper_file_name());
    helper.is_file().then_some(helper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_passes_live_beside_the_sandboxed_sidecar_folder_not_inside_it() {
        let data = Path::new("/d");
        let passes = passes_dir(data);
        assert_eq!(passes, Path::new("/d/cloudflare_passes"));
        assert!(!passes.starts_with(data.join("sidecar")));
    }

    #[test]
    fn saved_passes_open_a_private_folder_and_report_a_blocked_one() {
        let data = std::env::temp_dir().join(format!("cs_wiring_passes_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).expect("data");
        assert!(saved_passes(&data).is_some());
        assert!(data.join("cloudflare_passes").is_dir());
        let blocked = data.join("file");
        std::fs::write(&blocked, "x").expect("file");
        assert!(
            saved_passes(&blocked).is_none(),
            "a data folder that is a file turns the feature off"
        );
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn helper_args_name_both_folders() {
        let args = helper_args(
            Path::new("/d/sidecar/cloudflare"),
            Path::new("/d/cloudflare_browser"),
        );
        assert_eq!(
            args,
            [
                "--exchange",
                "/d/sidecar/cloudflare",
                "--profile",
                "/d/cloudflare_browser"
            ]
        );
    }
}
